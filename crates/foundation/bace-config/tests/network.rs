use bace_config::ServerConfig;

#[test]
fn existing_configs_get_requested_defaults_and_can_override() {
    let base = include_str!("../../../../tests/fixtures/config/server.toml");
    let original = ServerConfig::parse(base).unwrap();
    assert!(original.accounts.allow_auto_creation);
    assert!(!original.dat_distribution.enabled);
    let changed =
        ServerConfig::parse(&format!("{base}\n[accounts]\nallow_auto_creation=false\n")).unwrap();
    assert!(!changed.accounts.allow_auto_creation);
    assert!(
        ServerConfig::parse(&format!("{base}\n[network]\nauthentication_workers=0\n")).is_err()
    );
    assert!(ServerConfig::parse(&format!("{base}\n[network]\nunknown_limit=10\n")).is_err());
}

#[test]
fn distributing_assets_requires_a_source_directory() {
    let mut config = ServerConfig::default();
    config.dat_distribution.enabled = true;
    assert!(config.validate().is_err());
    config.dat_directory = Some("operator-supplied".into());
    assert!(config.validate().is_ok());
}
