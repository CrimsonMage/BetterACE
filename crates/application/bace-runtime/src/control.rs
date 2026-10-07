//! Portable private child control. The capability never crosses the socket;
//! both endpoints prove knowledge using fresh, role-separated HMAC challenges.
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const FRAME_LIMIT: usize = 65536;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChildMetadata {
    pub version: u32,
    pub port: u16,
    pub generation: u64,
    pub pid: u32,
    pub capability: [u8; 32],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    version: u32,
    generation: u64,
    nonce: [u8; 32],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    nonce: [u8; 32],
    tag: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChildRequest {
    Status,
    Drain { operation_id: u64 },
    Exit { operation_id: u64 },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChildReply {
    Status {
        game_ready: bool,
        detail: String,
        operation_id: Option<u64>,
    },
    Drained {
        operation_id: u64,
    },
    Blocked {
        operation_id: u64,
        detail: String,
    },
    Exiting {
        operation_id: u64,
    },
}
pub(crate) fn random_bytes() -> io::Result<[u8; 32]> {
    let mut bytes = [0; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| io::Error::other("secure random source unavailable"))?;
    Ok(bytes)
}
fn mac(
    key: &[u8; 32],
    generation: u64,
    client: &[u8; 32],
    server: &[u8; 32],
    role: &[u8],
) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts a 32-byte key");
    mac.update(b"BACE-control-v1");
    mac.update(&generation.to_le_bytes());
    mac.update(client);
    mac.update(server);
    mac.update(role);
    mac
}
pub(crate) async fn client_auth(
    stream: &mut TcpStream,
    metadata: &ChildMetadata,
) -> io::Result<()> {
    let client = random_bytes()?;
    write_frame(
        stream,
        &Hello {
            version: 1,
            generation: metadata.generation,
            nonce: client,
        },
    )
    .await?;
    let proof: Proof = read_frame(stream).await?;
    mac(
        &metadata.capability,
        metadata.generation,
        &client,
        &proof.nonce,
        b"server",
    )
    .verify_slice(&proof.tag)
    .map_err(|_| io::Error::other("child authentication failed"))?;
    let tag = mac(
        &metadata.capability,
        metadata.generation,
        &client,
        &proof.nonce,
        b"client",
    )
    .finalize()
    .into_bytes()
    .to_vec();
    write_frame(stream, &Proof { nonce: client, tag }).await
}
pub(crate) async fn server_auth(
    stream: &mut TcpStream,
    metadata: &ChildMetadata,
) -> io::Result<()> {
    let hello: Hello = read_frame(stream).await?;
    if hello.version != 1 || hello.generation != metadata.generation {
        return Err(io::Error::other("unsupported control generation"));
    }
    let server = random_bytes()?;
    let tag = mac(
        &metadata.capability,
        metadata.generation,
        &hello.nonce,
        &server,
        b"server",
    )
    .finalize()
    .into_bytes()
    .to_vec();
    write_frame(stream, &Proof { nonce: server, tag }).await?;
    let proof: Proof = read_frame(stream).await?;
    if proof.nonce != hello.nonce {
        return Err(io::Error::other("control nonce mismatch"));
    }
    mac(
        &metadata.capability,
        metadata.generation,
        &hello.nonce,
        &server,
        b"client",
    )
    .verify_slice(&proof.tag)
    .map_err(|_| io::Error::other("supervisor authentication failed"))
}
pub(crate) async fn read_frame<T: DeserializeOwned>(stream: &mut TcpStream) -> io::Result<T> {
    let length = stream.read_u32_le().await? as usize;
    if length == 0 || length > FRAME_LIMIT {
        return Err(io::Error::other("control frame exceeds limit"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).map_err(|_| io::Error::other("invalid control frame"))
}
pub(crate) async fn write_frame<T: Serialize>(stream: &mut TcpStream, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > FRAME_LIMIT {
        return Err(io::Error::other("control frame exceeds limit"));
    }
    stream.write_u32_le(bytes.len() as u32).await?;
    stream.write_all(&bytes).await
}
pub(crate) async fn connect(metadata: &ChildMetadata) -> io::Result<TcpStream> {
    tokio::time::timeout(Duration::from_secs(3), async {
        if metadata.version != 1 || metadata.port == 0 {
            return Err(io::Error::other("invalid child metadata"));
        }
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, metadata.port)).await?;
        client_auth(&mut stream, metadata).await?;
        Ok(stream)
    })
    .await
    .map_err(|_| io::Error::other("control authentication timed out"))?
}
pub(crate) fn lock(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    file.try_lock()
        .map_err(|_| io::Error::other("another process owns this host/child state"))?;
    Ok(file)
}
pub(crate) fn store_metadata(path: &Path, metadata: &ChildMetadata) -> io::Result<()> {
    // Called only while holding the child lock. Stale metadata does not own a child.
    if path.exists() {
        fs::remove_file(path)?;
    }
    let mut options = File::options();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(metadata).map_err(io::Error::other)?)?;
    file.sync_all()
}
pub(crate) fn load_metadata(path: &Path) -> io::Result<ChildMetadata> {
    let info = fs::symlink_metadata(path)?;
    if !info.is_file() || info.len() > 2048 {
        return Err(io::Error::other("invalid child metadata"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if info.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::other(
                "child capability permissions are not private",
            ));
        }
    }
    serde_json::from_slice(&fs::read(path)?).map_err(|_| io::Error::other("invalid child metadata"))
}

#[cfg(test)]
mod tests;
