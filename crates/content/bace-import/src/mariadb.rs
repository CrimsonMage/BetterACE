use crate::{SqlStagingBackend, SqlStagingError, StagedWorld};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

#[derive(Clone, Debug)]
pub struct MariaDbBinaries {
    pub basedir: PathBuf,
    pub install_db: PathBuf,
    pub server: PathBuf,
    pub client: PathBuf,
    /// Optional private library directory for an unpacked distribution package.
    pub library_dir: Option<PathBuf>,
}

/// Owns no externally supplied socket/database. Each call launches a fresh,
/// private, network-disabled server and destroys it when the call completes.
pub struct MariaDbStaging {
    binaries: MariaDbBinaries,
}
impl MariaDbStaging {
    pub fn new(binaries: MariaDbBinaries) -> Self {
        Self { binaries }
    }
}

pub(crate) struct IsolatedMariaDb {
    pub(crate) directory: TempDir,
    binaries: MariaDbBinaries,
    child: Child,
    socket: PathBuf,
}
impl Drop for IsolatedMariaDb {
    fn drop(&mut self) {
        crate::sql_process::terminate(&mut self.child);
    }
}

fn failure(error: impl std::fmt::Display) -> SqlStagingError {
    SqlStagingError::Extraction(error.to_string())
}
fn configure(command: &mut Command, binaries: &MariaDbBinaries) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if let Some(path) = &binaries.library_dir {
        command.env("LD_LIBRARY_PATH", path);
    }
}

impl IsolatedMariaDb {
    fn start(binaries: &MariaDbBinaries) -> Result<Self, SqlStagingError> {
        for binary in [&binaries.install_db, &binaries.server, &binaries.client] {
            if !binary.is_file() {
                return Err(SqlStagingError::Unavailable(format!(
                    "required private MariaDB binary missing: {}",
                    binary.display()
                )));
            }
        }
        let directory = tempfile::Builder::new()
            .prefix("bace-import-")
            .tempdir()
            .map_err(failure)?;
        let data = directory.path().join("data");
        let socket = directory.path().join("server.sock");
        let temporary = directory.path().join("tmp");
        std::fs::create_dir(&temporary).map_err(failure)?;
        let init_stdout = directory.path().join("initialize.out");
        let init_stderr = directory.path().join("initialize.err");
        let mut initialize = Command::new(&binaries.install_db);
        configure(&mut initialize, binaries);
        initialize.env("TMPDIR", &temporary);
        initialize
            .arg("--no-defaults")
            .arg(format!("--tmpdir={}", temporary.display()))
            .arg(format!("--basedir={}", binaries.basedir.display()))
            .arg(format!("--datadir={}", data.display()))
            .arg("--auth-root-authentication-method=normal")
            .arg("--skip-test-db");
        initialize
            .stdout(File::create(&init_stdout).map_err(failure)?)
            .stderr(File::create(&init_stderr).map_err(failure)?);
        let mut initializer = initialize.spawn().map_err(failure)?;
        let status = crate::sql_process::wait_bounded(
            &mut initializer,
            &init_stdout,
            &init_stderr,
            1024 * 1024,
            Duration::from_secs(30),
        )?;
        if !status.success() {
            return Err(failure(format!(
                "private MariaDB initialization failed: {} {}",
                std::fs::read_to_string(&init_stdout).unwrap_or_default(),
                std::fs::read_to_string(&init_stderr).unwrap_or_default()
            )));
        }
        let mut server = Command::new(&binaries.server);
        configure(&mut server, binaries);
        server.env("TMPDIR", &temporary);
        server
            .arg("--no-defaults")
            .arg(format!("--basedir={}", binaries.basedir.display()))
            .arg(format!("--datadir={}", data.display()))
            .arg(format!("--socket={}", socket.display()))
            .arg(format!(
                "--pid-file={}",
                directory.path().join("server.pid").display()
            ))
            .arg(format!(
                "--log-error={}",
                directory.path().join("server.log").display()
            ))
            .arg(format!("--secure-file-priv={}", directory.path().display()))
            .arg(format!("--tmpdir={}", temporary.display()))
            .arg("--skip-networking")
            .arg("--local-infile=0")
            .arg("--innodb-buffer-pool-size=32M")
            .arg("--max-connections=4")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = server.spawn().map_err(failure)?;
        let mut instance = Self {
            directory,
            binaries: binaries.clone(),
            child,
            socket,
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if instance.socket.exists() && instance.query("SELECT 1").is_ok() {
                break;
            }
            if instance.child.try_wait().map_err(failure)?.is_some() || Instant::now() >= deadline {
                let log = std::fs::read_to_string(instance.directory.path().join("server.log"))
                    .unwrap_or_default();
                return Err(failure(format!(
                    "private MariaDB failed to become ready: {log}"
                )));
            }
            thread::sleep(Duration::from_millis(25));
        }
        Ok(instance)
    }
    fn client(&self) -> Command {
        let mut command = Command::new(&self.binaries.client);
        configure(&mut command, &self.binaries);
        command
            .arg("--no-defaults")
            .arg("--protocol=socket")
            .arg(format!("--socket={}", self.socket.display()))
            .arg("--user=root")
            .arg("--batch")
            .arg("--raw")
            .arg("--skip-column-names")
            .arg("--default-character-set=utf8mb4")
            .arg("--local-infile=0")
            .arg("--binary-mode");
        command
    }
    pub(crate) fn query(&self, sql: &str) -> Result<String, SqlStagingError> {
        let stdout = self.directory.path().join("query.out");
        let stderr = self.directory.path().join("query.err");
        let mut child = self
            .client()
            .arg("--execute")
            .arg(sql)
            .stdout(File::create(&stdout).map_err(failure)?)
            .stderr(File::create(&stderr).map_err(failure)?)
            .spawn()
            .map_err(failure)?;
        let status = crate::sql_process::wait_bounded(
            &mut child,
            &stdout,
            &stderr,
            256 * 1024 * 1024,
            Duration::from_secs(60),
        )?;
        if !status.success() {
            return Err(failure(format!(
                "SQL staging query failed: {}",
                std::fs::read_to_string(stderr).unwrap_or_default()
            )));
        }
        std::fs::read_to_string(stdout).map_err(failure)
    }
    fn load(&self, path: &Path, database: bool) -> Result<(), SqlStagingError> {
        let stderr =
            File::create(self.directory.path().join("client-error.log")).map_err(failure)?;
        let mut command = self.client();
        let stdout_path = self.directory.path().join("client-output.log");
        if database {
            command.arg("--show-warnings").arg("--init-command=SET SESSION sql_mode='STRICT_ALL_TABLES,NO_ENGINE_SUBSTITUTION'").arg("ace_world");
        }
        let mut child = command
            .stdin(File::open(path).map_err(failure)?)
            .stdout(File::create(&stdout_path).map_err(failure)?)
            .stderr(stderr)
            .spawn()
            .map_err(failure)?;
        let status = crate::sql_process::wait_bounded(
            &mut child,
            &stdout_path,
            &self.directory.path().join("client-error.log"),
            1024 * 1024,
            Duration::from_secs(120),
        )?;
        if !status.success() {
            return Err(failure(format!(
                "SQL input failed: {}",
                std::fs::read_to_string(self.directory.path().join("client-error.log"))
                    .unwrap_or_default()
            )));
        }
        if database {
            let mut diagnostics = String::new();
            File::open(stdout_path)
                .map_err(failure)?
                .take(1024 * 1024 + 1)
                .read_to_string(&mut diagnostics)
                .map_err(failure)?;
            if diagnostics.len() > 1024 * 1024
                || diagnostics.lines().any(|line| line.starts_with("Warning ") && line != "Warning (Code 1287): '@@sql_notes' is deprecated and will be removed in a future release. Please use '@@note_verbosity' instead")
            {
                return Err(failure(
                    format!("SQL input generated warnings or excessive output; possible lossy conversion rejected: {}", diagnostics.chars().take(8192).collect::<String>()),
                ));
            }
        }
        Ok(())
    }
}
impl SqlStagingBackend for MariaDbStaging {
    fn extract_isolated(&mut self, dump: &Path) -> Result<StagedWorld, SqlStagingError> {
        self.with_loaded(dump, crate::sql_extract::extract)
    }
}
impl MariaDbStaging {
    pub(crate) fn with_loaded<T>(
        &mut self,
        dump: &Path,
        extract: impl FnOnce(&IsolatedMariaDb, String) -> Result<T, SqlStagingError>,
    ) -> Result<T, SqlStagingError> {
        let instance = IsolatedMariaDb::start(&self.binaries)?;
        let schema_path = instance.directory.path().join("schema.sql");
        File::create(&schema_path)
            .and_then(|mut file| file.write_all(include_bytes!("../data/world-base.sql")))
            .map_err(failure)?;
        instance.load(&schema_path, false)?;
        // WorldBase.sql still declares a signed motion column; the pinned
        // current ACE model declares uint?. Preserve the model's full range.
        instance.query("ALTER TABLE ace_world.weenie_properties_emote_action MODIFY motion INT UNSIGNED NULL DEFAULT NULL")?;
        // Load exactly the private immutable snapshot whose digest is recorded.
        let snapshot_path = instance.directory.path().join("input.sql");
        let mut snapshot = File::create(&snapshot_path).map_err(failure)?;
        let mut total = 0_usize;
        let mut digest = Sha256::new();
        let mut input = File::open(dump).map_err(failure)?;
        let mut buffer = [0_u8; 65536];
        loop {
            let length = input.read(&mut buffer).map_err(failure)?;
            if length == 0 {
                break;
            }
            total = total.saturating_add(length);
            if total > 512 * 1024 * 1024 {
                return Err(failure("SQL source exceeds 512 MiB import limit"));
            }
            digest.update(&buffer[..length]);
            snapshot.write_all(&buffer[..length]).map_err(failure)?;
        }
        drop(snapshot);
        instance.load(&snapshot_path, true)?;
        extract(&instance, format!("{:x}", digest.finalize()))
    }
}
