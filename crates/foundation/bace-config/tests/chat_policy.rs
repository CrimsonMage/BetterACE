use bace_config::ServerConfig;
#[test]
fn ace_chat_defaults_and_typed_overrides() {
    let default = ServerConfig::default();
    assert!(default.chat_policy.inform_reject);
    assert!(!default.chat_policy.echo_only);
    assert_eq!(default.chat_policy.requires_account_time_seconds, 0);
    let source = "bind_address='127.0.0.1:9000'\ndatabase_url_env='DB'\ncommand_capacity=16\nmax_sessions=4\ndatabase_connections=2\n[chat_policy]\nrequires_account_time_seconds=100\nrequires_player_age=50\nrequires_player_level=10\necho_reject=true\n";
    let parsed = ServerConfig::parse(source).unwrap();
    assert_eq!(parsed.chat_policy.requires_account_time_seconds, 100);
    assert_eq!(parsed.chat_policy.requires_player_age, 50);
    assert!(parsed.chat_policy.echo_reject);
    assert!(
        ServerConfig::parse(&source.replace("requires_player_age=50", "requires_player_age=-1"))
            .is_err()
    );
    assert!(
        ServerConfig::parse(&source.replace("echo_reject=true", "invented_throttle=1")).is_err()
    );
}
