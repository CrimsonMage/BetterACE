//! Pinned SentinelCommands.HandleBoot: private result, account boot, Audit.
use super::*;
use bace_admin::{BootSelector, StaffOperation};
use bace_auth::{AccessLevel, StaffPrincipal};

impl GameRuntime {
    pub(in crate::game_runtime::staff) fn apply_staff_boot(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        command: &bace_admin::AuthorizedCommand,
        principal: StaffPrincipal,
        token: u64,
    ) -> Result<bool, String> {
        if command.spec.name != "boot" {
            return Ok(false);
        }
        let Some(StaffOperation::Boot {
            selector,
            name,
            reason,
        }) = bace_admin::prepare_staff_operation(command)
            .map_err(|error| format!("staff boot parameters: {error:?}"))?
        else {
            return Err("staff boot operation mismatch".into());
        };
        let issuer = self.sessions.get(&key).ok_or("staff boot issuer missing")?;
        let loaded = issuer
            .loading
            .as_ref()
            .ok_or("staff boot issuer not entered")?;
        if issuer.terminated
            || issuer.disconnected
            || loaded.loaded.binding.actor != context.actor
            || loaded.loaded.binding.account != context.account
            || loaded.loaded.binding.session != context.session
            || issuer.account.access_level != principal.account_access
            || !principal.allows(AccessLevel::Sentinel, command.sudo)
        {
            return Err("staff boot issuer authorization changed".into());
        }
        let sender = loaded.loaded.player.player.name.clone();
        let Some(replica) = self.players.replication(context.actor) else {
            return Err("staff boot issuer replication missing".into());
        };
        if replica.key != key || replica.binding != loaded.loaded.binding {
            return Err("staff boot issuer recipient changed".into());
        }
        let (target, message) = match selector {
            BootSelector::Invalid => (
                None,
                "You must specify what you are booting with char, account, or iid as the first parameter.".to_owned(),
            ),
            BootSelector::Instance if parse_iid(&name).is_none() => (
                None,
                "That is not a valid Instance ID (IID). IIDs must be between 0x50000001 and 0x5FFFFFFF".to_owned(),
            ),
            selector => {
                let description = match selector {
                    BootSelector::Character => "character",
                    BootSelector::Account => "account",
                    BootSelector::Instance => "instance id",
                    BootSelector::Invalid => unreachable!(),
                };
                let target = self.sessions.iter().find_map(|(candidate, session)| {
                    if session.terminated || session.disconnected || !session.connected {
                        return None;
                    }
                    let matches = match selector {
                        BootSelector::Character => session.loading.as_ref().is_some_and(|loading| {
                            loading.loaded.player.player.name.eq_ignore_ascii_case(&name)
                        }),
                        BootSelector::Account => session.account.name.as_str().eq_ignore_ascii_case(&name),
                        BootSelector::Instance => session.loading.as_ref().is_some_and(|loading| {
                            Some(loading.loaded.binding.actor.0) == parse_iid(&name)
                        }),
                        BootSelector::Invalid => false,
                    };
                    matches.then_some(*candidate)
                });
                let message = if target.is_some() {
                    format!(
                        "Booting {description} {name}.{}",
                        reason.as_ref().map_or(String::new(), |reason| format!(" Reason: {reason}"))
                    )
                } else {
                    format!("Cannot boot \"{name}\" because that {description} is not currently online or cannot be found. Check syntax/spelling and try again.")
                };
                (target, message)
            }
        };
        let limits = bace_replication::BatchLimits {
            max_messages: 1,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4096,
        };
        let response =
            bace_replication::project_staff_response(replica.binding, &message, 0, limits)
                .map_err(|error| format!("staff boot response: {error:?}"))?;
        let response = crate::game_messages::session_batch_command(key, response)
            .map_err(|error| error.to_string())?;
        let boot = target
            .map(|target| {
                let reason = reason
                    .as_deref()
                    .map_or(String::new(), |reason| format!(" - {reason}"));
                bace_wire::AccountControl::Boot {
                    reason: Some(&reason),
                }
                .encode()
                .map(|bytes| (target, bytes))
                .map_err(|error| format!("staff boot packet: {error:?}"))
            })
            .transpose()?;
        if self.staff.output.len() + 1 + usize::from(boot.is_some()) > self.limits.messages {
            return Err("staff boot output capacity".into());
        }
        self.staff.output.push_back(response);
        if let Some((target, bytes)) = boot {
            // The accepted output order matches ACE: issuer result first, then
            // the target's final reliable boot message and disconnect.
            self.staff
                .output
                .push_back(NetworkCommand::TerminateAfterFlush {
                    key: target,
                    queue: 9,
                    bytes,
                });
            let target_session = self.sessions.get_mut(&target).expect("selected session");
            target_session.terminated = true;
            target_session.closing = true;
            self.staff.pending = Some(Pending {
                key,
                context,
                token,
                phase: Phase::BootAudit {
                    sender,
                    text: message,
                },
            });
        }
        Ok(true)
    }

    pub(in crate::game_runtime::staff) fn poll_staff_boot_audit(&mut self) -> Result<(), String> {
        let Some(Pending {
            context,
            token,
            phase: Phase::BootAudit { sender, text },
            ..
        }) = &self.staff.pending
        else {
            return Ok(());
        };
        let unix_seconds = self
            .clock
            .unix_millis
            .checked_add(
                u64::try_from(self.last_elapsed.as_millis())
                    .map_err(|_| "staff boot audit clock overflow")?,
            )
            .ok_or("staff boot audit clock overflow")?
            / 1000;
        let unix_seconds =
            i64::try_from(unix_seconds).map_err(|_| "staff boot audit clock overflow")?;
        if self
            .shard
            .publish_staff_audit(*token, unix_seconds, context.actor, sender, text)?
        {
            self.staff.pending = None;
        }
        Ok(())
    }
}

fn parse_iid(value: &str) -> Option<u32> {
    let hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    u32::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_iid_requires_prefixed_player_identity() {
        assert_eq!(parse_iid("0x50000001"), Some(0x5000_0001));
        assert_eq!(parse_iid("0X5FFFFFFF"), Some(0x5FFF_FFFF));
        for invalid in ["50000001", "0xnope", "0x100000000"] {
            assert_eq!(parse_iid(invalid), None);
        }
        // ACE parses any prefixed uint; a nonplayer IID simply misses online lookup.
        assert_eq!(parse_iid("0x60000000"), Some(0x6000_0000));
    }
}
