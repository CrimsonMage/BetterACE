use super::*;
#[test]
fn child_uses_the_exact_validated_native_configuration_without_credential_values() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = ServerConfig {
        bind_address: "127.0.0.1:9123".into(),
        max_sessions: 37,
        dat_directory: Some("approved-assets".into()),
        database_url_env: "BETTERACE_TEST_DATABASE".into(),
        ..Default::default()
    };
    config.accounts.allow_auto_creation = false;
    config.chat_policy.disable_general = true;
    let path = store_child_config(directory.path(), &config).unwrap();
    let loaded = ServerConfig::load(&path).unwrap();
    assert_eq!(
        toml::to_string(&loaded).unwrap(),
        toml::to_string(&config).unwrap()
    );
    assert_eq!(loaded.max_sessions, 37);
    assert_eq!(loaded.database_url_env, "BETTERACE_TEST_DATABASE");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    config.max_sessions = 38;
    store_child_config(directory.path(), &config).unwrap();
    assert_eq!(ServerConfig::load(&path).unwrap().max_sessions, 38);
}
#[cfg(unix)]
#[test]
fn generated_child_config_never_follows_an_existing_symbolic_link() {
    let directory = tempfile::tempdir().unwrap();
    let other = directory.path().join("unrelated");
    std::fs::write(&other, b"keep me").unwrap();
    std::os::unix::fs::symlink(&other, directory.path().join("child-config.toml")).unwrap();
    assert!(store_child_config(directory.path(), &ServerConfig::default()).is_err());
    assert_eq!(std::fs::read(&other).unwrap(), b"keep me");
}
