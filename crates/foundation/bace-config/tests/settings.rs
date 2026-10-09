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
fn world_name_defaults_for_existing_files_and_validates_wire_safe_text() {
    let source = include_str!("../../../../tests/fixtures/config/server.toml");
    assert_eq!(ServerConfig::parse(source).unwrap().world_name, "BetterACE");
    let named = format!("{source}\nworld_name = \"My World\"\n");
    assert_eq!(ServerConfig::parse(&named).unwrap().world_name, "My World");
    for invalid in ["", " ", "World\nName", "Café", &"x".repeat(129)] {
        let mut config = ServerConfig::parse(source).unwrap();
        config.world_name = invalid.into();
        assert!(config.validate().is_err(), "{invalid:?}");
    }
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
