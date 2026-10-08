//! Cold configuration projection; policy decisions remain in bace-social.
pub fn prepare_chat_policy(config: &bace_config::ChatPolicyConfig) -> bace_social::ChatPolicy {
    bace_social::ChatPolicy {
        disable_general: config.disable_general,
        disable_trade: config.disable_trade,
        disable_lfg: config.disable_lfg,
        disable_roleplay: config.disable_roleplay,
        disable_olthoi: config.disable_olthoi,
        echo_only: config.echo_only,
        echo_reject: config.echo_reject,
        inform_reject: config.inform_reject,
        requires_account_15days: config.requires_account_15days,
        requires_account_time_seconds: config.requires_account_time_seconds,
        requires_player_age: config.requires_player_age,
        requires_player_level: config.requires_player_level,
    }
}
