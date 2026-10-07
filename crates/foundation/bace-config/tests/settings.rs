use bace_config::ServerConfig;

#[test]
fn rejects_unknown_keys_unbounded_queues_and_invalid_port_pair() {
    let source = include_str!("../../../../tests/fixtures/config/server.toml");
    // Config parsing is strict to catch typos rather than silently ignore them.
    assert!(ServerConfig::parse(source).is_ok());
    assert!(ServerConfig::parse(&format!("{source}\ncommmand_capacity=4")).is_err());
    assert!(ServerConfig::parse(&source.replace("4096", "0")).is_err());
    assert!(ServerConfig::parse(&source.replace(":9000", ":65535")).is_err());
}

#[test]
fn host_defaults_are_local_and_resource_limits_are_validated() {
    let mut config = bace_config::HostConfig::default();
    assert_eq!(config.bind_address.to_string(), "127.0.0.1:8080");
    assert!(config.validate().is_ok());
    config.bind_address = "0.0.0.0:8080".parse().unwrap();
    assert!(config.validate().is_err());
    config.bind_address = "127.0.0.1:0".parse().unwrap();
    assert!(config.validate().is_err());
    config.bind_address = "127.0.0.1:8080".parse().unwrap();
    config.max_sessions = 100000;
    assert!(config.validate().is_err());
}
