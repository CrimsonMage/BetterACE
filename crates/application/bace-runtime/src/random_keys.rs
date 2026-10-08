//! Key material is provisioned/read on an I/O adapter, never inside a tick.
//! Bind the fingerprint in PostgreSQL before admitting any random gameplay.
use bace_random::RandomRoot;
#[cfg(unix)]
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::{fs::OpenOptions, io::Write};
use std::{io::Read, path::Path};

pub struct LoadedRandomKey {
    pub root: RandomRoot,
    pub fingerprint: [u8; 32],
}

pub fn load_random_key(path: &Path) -> Result<LoadedRandomKey, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != 76 {
        return Err("invalid random key file length".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err("random key file must be private to its owner".into());
        }
    }
    let mut bytes = [0u8; 76];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    if &bytes[..8] != b"BACERNG1" || Sha256::digest(&bytes[..44]).as_slice() != &bytes[44..] {
        return Err("random key integrity check failed".into());
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| "key version")?);
    let key: [u8; 32] = bytes[12..44].try_into().map_err(|_| "key length")?;
    let fingerprint = Sha256::digest(&bytes[..44]).into();
    let root = RandomRoot::new(key, version).map_err(|_| "invalid random key version")?;
    bytes.fill(0);
    Ok(LoadedRandomKey { root, fingerprint })
}

/// Fresh installation only. Existing material is never replaced or reseeded.
/// On Windows, provision a private key through the deployment's ACL-controlled
/// secret provider; this optional filesystem provisioning helper is Unix-only.
pub fn initialize_random_key(path: &Path, version: u32) -> Result<LoadedRandomKey, String> {
    if version == 0 {
        return Err("random key version must be nonzero".into());
    }
    if path.exists() {
        let key = load_random_key(path)?;
        if key.root.key_version() != version {
            return Err("existing key version differs".into());
        }
        return Ok(key);
    }
    #[cfg(not(unix))]
    {
        return Err(
            "automatic random-key file provisioning requires a qualified private-file provider"
                .into(),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut bytes = [0u8; 76];
        bytes[..8].copy_from_slice(b"BACERNG1");
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        OsRng
            .try_fill_bytes(&mut bytes[12..44])
            .map_err(|_| "OS entropy unavailable")?;
        let digest = Sha256::digest(&bytes[..44]);
        bytes[44..].copy_from_slice(&digest);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::File::open(parent)
                .and_then(|d| d.sync_all())
                .map_err(|e| e.to_string())?;
        }
        bytes.fill(0);
        load_random_key(path)
    }
}
