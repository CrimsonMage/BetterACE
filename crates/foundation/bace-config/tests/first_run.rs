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
fn creates_readable_valid_template_with_gameplay_inputs_unconfigured() {
    let directory = TestDirectory::new();
    let path = directory.path().join("server.toml");
    let (config, created) = ServerConfig::load_or_create_default(&path).unwrap();

    assert!(created);
    assert!(config.dat_directory.is_none());
    assert!(config.pack_directory.is_none());
    assert!(config.random_key_file.is_none());
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(source.contains("# dat_directory ="));
    assert!(source.contains("# pack_directory ="));
    assert!(source.contains("# random_key_file ="));
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
        assert!(config.dat_directory.is_none());
        creations += usize::from(created);
    }
    assert_eq!(creations, 1);
    assert!(ServerConfig::load(&path).is_ok());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
