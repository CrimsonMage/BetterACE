use super::*;

#[test]
fn implicit_host_and_serve_select_default_config() {
    let host = Arguments::try_parse_from(["bace-server"]).unwrap();
    assert!(host.command.is_none());
    let serve = Arguments::try_parse_from(["bace-server", "serve"]).unwrap();
    assert!(matches!(
        serve.command,
        Some(Command::Serve { config: None })
    ));

    let directory = tempfile::tempdir().unwrap();
    let default_path = directory.path().join("server.toml");
    let (config, created) = load_run_config(None, &default_path).unwrap();
    assert!(created);
    assert_eq!(config.bind_address, "127.0.0.1:9000");
    assert!(default_path.is_file());
    let (_, created) = load_run_config(None, &default_path).unwrap();
    assert!(!created);
}

#[test]
fn explicit_config_path_is_read_only_and_missing_path_is_an_error() {
    let serve =
        Arguments::try_parse_from(["bace-server", "serve", "--config", "custom.toml"]).unwrap();
    assert!(matches!(
        serve.command,
        Some(Command::Serve { config: Some(path) }) if path == Path::new("custom.toml")
    ));
    let directory = tempfile::tempdir().unwrap();
    let default_path = directory.path().join("server.toml");
    let missing = directory.path().join("misspelled.toml");
    assert!(load_run_config(Some(&missing), &default_path).is_err());
    assert!(!missing.exists());
    assert!(!default_path.exists());
}
