//! Explicit-clock policy from ACE TurbineChatHandler and PropertyManager defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatPolicy {
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
impl Default for ChatPolicy {
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatEligibility {
    pub account_created_unix: Option<i64>,
    pub account_15_days: bool,
    pub player_age_seconds: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicChatGate {
    Deliver,
    Echo,
    Reject { reason: String, inform: bool },
}
impl ChatPolicy {
    pub fn evaluate(
        &self,
        original_channel: u32,
        adjusted_channel: u32,
        eligibility: ChatEligibility,
        level: u32,
        now: i64,
    ) -> PublicChatGate {
        use PublicChatGate::*;
        let reject = |reason: String, inform| Reject { reason, inform };
        if adjusted_channel == 10 {
            if self.disable_olthoi {
                return reject(String::new(), self.inform_reject);
            }
            return if self.echo_only { Echo } else { Deliver };
        }
        if !(2..=5).contains(&adjusted_channel) {
            return Deliver;
        }
        if self.echo_only {
            return Echo;
        }
        if self.requires_account_15days && !eligibility.account_15_days {
            return reject(
                "because this account is not 15 days old".into(),
                self.inform_reject,
            );
        }
        if self.requires_account_time_seconds > 0
            && eligibility
                .account_created_unix
                .and_then(|created| now.checked_sub(created))
                .and_then(|age| u64::try_from(age).ok())
                .is_none_or(|age| age < self.requires_account_time_seconds)
        {
            return reject(
                "because this account is not old enough".into(),
                self.inform_reject,
            );
        }
        if eligibility.player_age_seconds < self.requires_player_age {
            return reject(
                "because this character has not been played enough".into(),
                self.inform_reject,
            );
        }
        if level < self.requires_player_level {
            return reject(
                format!(
                    "because this character has not reached level {}",
                    self.requires_player_level
                ),
                self.inform_reject,
            );
        }
        let disabled = match original_channel {
            2 => self.disable_general,
            3 => self.disable_trade,
            4 => self.disable_lfg,
            5 => self.disable_roleplay,
            _ => false,
        };
        if disabled {
            reject(String::new(), false)
        } else {
            Deliver
        }
    }
}
