use bace_auth::{PasswordHashRecord, PasswordService};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("host file operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("host credentials missing at {}; run bace-cli host-init for this state directory", .0.display())]
    MissingOperator(PathBuf),
    #[error("host credentials are invalid or unavailable; run bace-cli host-init")]
    Credentials,
    #[error("host state directory or file is not private")]
    Permissions,
    #[error(
        "Windows private-ACL provisioning has not yet been validated; host console provisioning is unsupported on this build"
    )]
    WindowsPermissionsUnsupported,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperatorFile {
    version: u32,
    password_hash: String,
}

/// Provision once, independent of game accounts/PostgreSQL. Existing credentials
/// are never replaced implicitly. Passwords MUST come from a hidden prompt/stdin.
pub fn provision_operator(path: &Path, password: &[u8]) -> Result<(), HostError> {
    let parent = path.parent().ok_or(HostError::Permissions)?;
    ensure_private_directory(parent)?;
    let hash = PasswordService::new(1)
        .map_err(|_| HostError::Credentials)?
        .hash(password)
        .map_err(|_| HostError::Credentials)?;
    let record = OperatorFile {
        version: 1,
        password_hash: hash.as_phc().to_owned(),
    };
    let source = toml::to_string(&record).map_err(|_| HostError::Credentials)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(source.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|error| HostError::Io(error.error))?;
    Ok(())
}
pub fn load_operator(path: &Path) -> Result<PasswordHashRecord, HostError> {
    ensure_private_directory(path.parent().ok_or(HostError::Permissions)?)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(HostError::MissingOperator(path.to_path_buf()));
        }
        Err(error) => return Err(HostError::Io(error)),
    };
    if !metadata.is_file() || metadata.len() > 2048 {
        return Err(HostError::Permissions);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(HostError::Permissions);
        }
    }
    let record: OperatorFile =
        toml::from_str(&fs::read_to_string(path)?).map_err(|_| HostError::Credentials)?;
    if record.version != 1 {
        return Err(HostError::Credentials);
    }
    PasswordHashRecord::parse(&record.password_hash).map_err(|_| HostError::Credentials)
}
pub fn ensure_private_directory(path: &Path) -> Result<(), HostError> {
    #[cfg(windows)]
    {
        let _ = path;
        return Err(HostError::WindowsPermissionsUnsupported);
    }
    #[cfg(not(windows))]
    {
        if !path.exists() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)?;
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() {
            return Err(HostError::Permissions);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(HostError::Permissions);
            }
        }
        Ok(())
    }
}
