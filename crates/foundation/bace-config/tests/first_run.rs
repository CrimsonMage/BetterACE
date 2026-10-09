use bace_config::ServerConfig;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        loop {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bace-config-first-run-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create test directory: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove test directory");
    }
}

#[test]
fn creates_readable_valid_template_with_conventional_gameplay_paths() {
    let directory = TestDirectory::new();
    let path = directory.path().join("server.toml");
    let (config, created) = ServerConfig::load_or_create_default(&path).unwrap();

    assert!(created);
    assert_eq!(config.dat_directory, Some(PathBuf::from("data/dats")));
    assert_eq!(
        config.pack_directory,
        Some(PathBuf::from(".local/world-packs"))
    );
    assert_eq!(
        config.random_key_file,
        Some(PathBuf::from("state/random-key"))
    );
    assert!(config.database_url.is_none());
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(source.contains("dat_directory = \"data/dats\""));
    assert!(source.contains("pack_directory = \".local/world-packs\""));
    assert!(source.contains("random_key_file = \"state/random-key\""));
    assert!(source.contains("# database_url ="));
    assert_eq!(ServerConfig::load(&path).unwrap().max_sessions, 512);
}

#[test]
fn repeated_load_preserves_operator_edits_byte_for_byte() {
    let directory = TestDirectory::new();
    let path = directory.path().join("server.toml");
    ServerConfig::load_or_create_default(&path).unwrap();
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("max_sessions = 512", "max_sessions = 33 # operator edit");
    std::fs::write(&path, &edited).unwrap();

    let (config, created) = ServerConfig::load_or_create_default(&path).unwrap();
    assert!(!created);
    assert_eq!(config.max_sessions, 33);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
}

#[test]
fn malformed_existing_file_is_reported_and_not_replaced() {
    let directory = TestDirectory::new();
    let path = directory.path().join("server.toml");
    let invalid = b"max_sessions = -999 # unfinished operator edit\n";
    std::fs::write(&path, invalid).unwrap();

    assert!(ServerConfig::load_or_create_default(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), invalid);
}

#[test]
fn simultaneous_first_runs_publish_one_complete_file() {
    let directory = TestDirectory::new();
    let path = directory.path().join("server.toml");
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                ServerConfig::load_or_create_default(&path)
            })
        })
        .collect();

    let mut creations = 0;
    for handle in handles {
        let (config, created) = handle.join().unwrap().unwrap();
        assert_eq!(config.dat_directory, Some(PathBuf::from("data/dats")));
        creations += usize::from(created);
    }
    assert_eq!(creations, 1);
    assert!(ServerConfig::load(&path).is_ok());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn database_url_falls_back_to_config_or_reports_missing() {
    let unique = format!(
        "BACE_TEST_MISSING_DATABASE_URL_{}_{}",
        std::process::id(),
        NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    );
    assert!(std::env::var_os(&unique).is_none());
    let mut config = ServerConfig {
        database_url_env: unique.clone(),
        ..ServerConfig::default()
    };
    assert_eq!(
        config.resolve_database_url().unwrap_err().to_string(),
        format!("{unique} is not set and database_url is not configured")
    );
    config.database_url = Some("postgresql:///configured".into());
    assert_eq!(
        config.resolve_database_url().unwrap(),
        "postgresql:///configured"
    );
    config.database_url = Some(" \t ".into());
    assert!(config.validate().is_err());
}

#[test]
fn database_url_environment_override_runs_in_child_process() {
    let unique = format!(
        "BACE_TEST_DATABASE_URL_{}_{}",
        std::process::id(),
        NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    );
    assert!(std::env::var_os(&unique).is_none());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "database_url_environment_override_child",
            "--nocapture",
        ])
        .env("BACE_TEST_DATABASE_URL_ENV_NAME", &unique)
        .env(&unique, "postgresql:///environment")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn database_url_environment_override_child() {
    let Ok(unique) = std::env::var("BACE_TEST_DATABASE_URL_ENV_NAME") else {
        return;
    };
    let config = ServerConfig {
        database_url_env: unique,
        database_url: Some("postgresql:///configured".into()),
        ..ServerConfig::default()
    };
    assert_eq!(
        config.resolve_database_url().unwrap(),
        "postgresql:///environment"
    );
}
