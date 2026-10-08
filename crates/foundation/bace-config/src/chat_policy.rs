//! Restart-only pinned ACE public chat policy. Chat API authorization is independent.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChatPolicyConfig {
    pub disable_general: bool,
    pub disable_trade: bool,
    pub disable_lfg: bool,
    pub disable_roleplay: bool,
    pub disable_olthoi: bool,
    pub echo_only: bool,
    pub echo_reject: bool,
    pub inform_reject: bool,
    pub requires_account_15days: bool,
    pub requires_account_time_seconds: u64,
    pub requires_player_age: u64,
    pub requires_player_level: u32,
}
impl Default for ChatPolicyConfig {
    fn default() -> Self {
        Self {
            disable_general: false,
            disable_trade: false,
            disable_lfg: false,
            disable_roleplay: false,
            disable_olthoi: false,
            echo_only: false,
            echo_reject: false,
            inform_reject: true,
            requires_account_15days: false,
            requires_account_time_seconds: 0,
            requires_player_age: 0,
            requires_player_level: 0,
        }
    }
}
