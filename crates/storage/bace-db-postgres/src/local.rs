#[cfg(unix)]
use crate::PgStore;
#[cfg(not(unix))]
use crate::StoreError;
use std::path::Path;

/// Development provisioning only. No existing cluster or system service is changed.
/// The private Unix socket is protected by the owning user's mode-0700 directory;
/// TCP is disabled. Other platforms can use an externally provisioned PostgreSQL.
#[cfg(unix)]
pub async fn initialize_local_database(
    directory: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, PermissionsExt},
        process::Command,
    };
    let marker = directory.join("betterace-local-v1");
    if directory.exists() && !marker.is_file() {
        return Err(
            "refusing to provision an existing directory without BetterACE's local database marker"
                .into(),
        );
    }
    if !directory.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        fs::write(&marker, b"BetterACE local PostgreSQL v1\n")?;
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let directory = directory.canonicalize()?;
    if directory.as_os_str().len() > 80 {
        return Err("local database path is too long for a portable Unix socket; choose a shorter --directory".into());
    }
    let data = directory.join("data");
    if !data.join("PG_VERSION").is_file() {
        run(Command::new("initdb")
            .args([
                "--no-locale",
                "-E",
                "UTF8",
                "-U",
                "betterace",
                "--auth-local=trust",
                "--auth-host=reject",
                "-D",
            ])
            .arg(&data))?;
    }
    let status = Command::new("pg_ctl")
        .arg("-D")
        .arg(&data)
        .arg("status")
        .output()?;
    if !status.status.success() {
        let quoted = format!("'{}'", directory.to_string_lossy().replace('\'', "'\\''"));
        let options = format!("-h '' -k {quoted} -c unix_socket_permissions=0700");
        run(Command::new("pg_ctl")
            .arg("-D")
            .arg(&data)
            .arg("-l")
            .arg(directory.join("postgres.log"))
            .args(["-o", &options, "-w", "start"]))?;
    }
    let host = directory.to_str().ok_or("non-UTF8 local database path")?;
    let encoded: String = host
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let admin_url = format!("postgresql:///postgres?host={encoded}&user=betterace");
    let admin = PgStore::connect(&admin_url, 1).await?;
    // Fixed identifier; never interpolate caller input into SQL.
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname='betterace')")
            .fetch_one(&admin.pool)
            .await?;
    if !exists {
        sqlx::query("CREATE DATABASE betterace")
            .execute(&admin.pool)
            .await?;
    }
    admin.close().await;
    let url = format!("postgresql:///betterace?host={encoded}&user=betterace");
    let store = PgStore::connect(&url, 2).await?;
    store.migrate().await?;
    store.close().await;
    Ok(url)
}

#[cfg(unix)]
fn run(command: &mut std::process::Command) -> Result<(), Box<dyn std::error::Error>> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "PostgreSQL provisioning failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

#[cfg(not(unix))]
pub async fn initialize_local_database(
    _directory: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    Err(StoreError::Invalid("local database provisioning needs Unix sockets; configure an external PostgreSQL URL on this platform").into())
}
