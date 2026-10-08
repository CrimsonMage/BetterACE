//! Retained shard commands, bounded broadcast fanout and actual lifecycle drains.
use super::*;
use crate::shard_control::*;
use bace_admin::{AuthorizedCommand, ChatPublisher, prepare_shard_command};
use bace_auth::{AccessLevel, StaffPrincipal};
use bace_gameplay_api::{
    ActionContext,
    social::{AcceptedChat, ChatChannel},
};
use bace_types::EntityId;

/// Host logging/audit owner must accept this record before acknowledging it.
/// Retaining an obligation is not a claim that a logger has written it.
#[derive(Clone, Debug)]
pub struct ShardHostRecord {
    pub event: ShardEvent,
    pub unix_millis: i64,
}
struct Fanout {
    sequence: u64,
    recipients: VecDeque<SessionKey>,
    bytes: Vec<u8>,
}
pub(super) struct ShardRuntime {
    control: ShardControl,
    publisher: Option<ChatPublisher>,
    host: VecDeque<ShardHostRecord>,
    fanout: Option<Fanout>,
}
impl ShardRuntime {
    pub(super) fn publish_staff_audit(
        &self,
        sequence: u64,
        unix_seconds: i64,
        actor: EntityId,
        sender_name: &str,
        text: &str,
    ) -> Result<bool, String> {
        let Some(publisher) = &self.publisher else {
            return Ok(true);
        };
        match publisher.publish(&AcceptedChat {
            sequence,
            unix_seconds,
            sender: actor,
            sender_name: sender_name.to_owned(),
            channel: ChatChannel::Audit,
            text: text.to_owned(),
        }) {
            Ok(()) => Ok(true),
            Err(bace_admin::FeedError::Full) => Ok(false),
            Err(error) => Err(format!("staff audit feed: {error:?}")),
        }
    }
    pub(super) fn terminal_stop_pending(&self) -> bool {
        self.control
            .peek_drain()
            .is_some_and(|r| r.kind == ShardDrainKind::StopWorld)
            && self.control.peek_event().is_none()
            && self.host.is_empty()
            && self.fanout.is_none()
    }
    pub(super) fn finish_world_stop(&mut self) -> Result<(), String> {
        if let Some(request) = self.control.peek_drain() {
            if !self.terminal_stop_pending() {
                return Err("shard still owns an earlier drain stage".into());
            }
            self.control
                .confirm_drain(ShardDrainReceipt {
                    request,
                    remaining: 0,
                    unsaved: 0,
                    pending_operations: 0,
                })
                .map_err(|e| format!("final shard stop receipt: {e:?}"))?;
        }
        Ok(())
    }
    pub(super) fn new(open: bool, publisher: Option<ChatPublisher>) -> Self {
        Self {
            control: ShardControl::new(256, open).expect("fixed shard capacity"),
            publisher,
            host: VecDeque::new(),
            fanout: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.control.has_pending() || !self.host.is_empty() || self.fanout.is_some()
    }
    pub(super) fn accepts(&self, access: AccessLevel) -> bool {
        !matches!(
            self.control.shutdown(),
            ShardShutdown::Draining(_) | ShardShutdown::Complete
        ) && (self.control.world_open() || access as u8 > AccessLevel::Player as u8)
    }
}
impl GameRuntime {
    pub fn shard_world_stop_requested(&self) -> bool {
        self.shard.terminal_stop_pending()
    }
    fn shard_clock(&self) -> Result<ShardClock, String> {
        let millis =
            u64::try_from(self.last_elapsed.as_millis()).map_err(|_| "shard clock overflow")?;
        Ok(ShardClock {
            monotonic_millis: millis,
            unix_millis: i64::try_from(
                self.clock
                    .unix_millis
                    .checked_add(millis)
                    .ok_or("shard clock overflow")?,
            )
            .map_err(|_| "shard clock overflow")?,
            local_offset_seconds: self.assets.local_offset_seconds,
        })
    }
    pub(super) fn apply_shard_command(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        command: &AuthorizedCommand,
        principal: StaffPrincipal,
    ) -> Result<bool, String> {
        let Some(command) =
            prepare_shard_command(command).map_err(|e| format!("shard command: {e:?}"))?
        else {
            return Ok(false);
        };
        let loaded = self
            .sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .ok_or("shard issuer missing")?;
        if loaded.loaded.binding.actor != context.actor
            || loaded.loaded.binding.account != context.account
            || loaded.loaded.binding.session != context.session
            || !self.players.entered(context.actor)
        {
            return Err("shard issuer binding mismatch".into());
        }
        let clock = self.shard_clock()?;
        self.shard
            .control
            .apply(
                &command,
                ShardIssuer::Player {
                    actor: context.actor,
                    name: &loaded.loaded.player.player.name,
                    principal,
                },
                clock,
            )
            .map_err(|e| format!("shard command: {e:?}"))?;
        Ok(true)
    }
    pub fn pending_shard_host_record(&self) -> Option<&ShardHostRecord> {
        self.shard.host.front()
    }
    pub fn acknowledge_shard_host_record(&mut self, sequence: u64) -> Result<(), String> {
        if self
            .shard
            .host
            .front()
            .is_none_or(|r| r.event.sequence != sequence)
        {
            return Err("shard host record correlation".into());
        }
        self.shard.host.pop_front();
        Ok(())
    }
    pub fn shard_shutdown(&self) -> ShardShutdown {
        self.shard.control.shutdown()
    }
    pub(super) fn poll_shard(&mut self) -> Result<(), String> {
        let clock = self.shard_clock()?;
        // Output pressure must not prevent accepted drains from progressing.
        let advance = self.shard.control.advance(clock);
        self.poll_shard_drain()?;
        self.poll_shard_output(clock)?;
        match advance {
            Ok(()) | Err(ShardError::Capacity) => Ok(()),
            Err(e) => Err(format!("shard clock: {e:?}")),
        }
    }
    fn poll_shard_output(&mut self, clock: ShardClock) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            if let Some(fanout) = &mut self.shard.fanout {
                if let Some(key) = fanout.recipients.front().copied() {
                    if self.network_output.len() >= self.limits.messages {
                        break;
                    }
                    // A disconnected generation cannot receive this accepted broadcast.
                    if self.sessions.get(&key).is_some_and(|s| !s.terminated) {
                        self.network_output.push_back(NetworkCommand::Send {
                            key,
                            queue: 9,
                            bytes: fanout.bytes.clone(),
                        });
                    }
                    fanout.recipients.pop_front();
                    continue;
                }
                self.shard
                    .control
                    .acknowledge_event(fanout.sequence)
                    .map_err(|e| format!("shard output: {e:?}"))?;
                self.shard.fanout = None;
                continue;
            }
            let Some(event) = self.shard.control.peek_event().cloned() else {
                break;
            };
            match &event.effect {
                ShardEffect::Log { .. }
                | ShardEffect::Audit { .. }
                | ShardEffect::Reply {
                    recipient: None, ..
                } => {
                    if self.shard.host.len() >= 256 {
                        break;
                    }
                    if let ShardEffect::Audit { actor, text } = &event.effect
                        && let Some(publisher) = &self.shard.publisher
                    {
                        let name = actor
                            .and_then(|actor| {
                                self.sessions
                                    .values()
                                    .filter_map(|s| s.loading.as_ref())
                                    .find(|l| l.loaded.binding.actor == actor)
                            })
                            .map_or("CONSOLE", |l| l.loaded.player.player.name.as_str());
                        match publisher.publish(&AcceptedChat {
                            sequence: event.sequence,
                            unix_seconds: clock.unix_millis / 1000,
                            sender: actor.unwrap_or(EntityId(0)),
                            sender_name: name.into(),
                            channel: ChatChannel::Audit,
                            text: text.clone(),
                        }) {
                            Ok(()) => {}
                            Err(bace_admin::FeedError::Full) => break,
                            Err(error) => return Err(format!("shard audit feed: {error:?}")),
                        }
                    }
                    self.shard.host.push_back(ShardHostRecord {
                        event: event.clone(),
                        unix_millis: clock.unix_millis,
                    });
                    self.shard
                        .control
                        .acknowledge_event(event.sequence)
                        .map_err(|e| format!("shard host output: {e:?}"))?;
                }
                ShardEffect::Reply {
                    recipient: Some(actor),
                    chat,
                    text,
                } => {
                    let recipients = self
                        .sessions
                        .iter()
                        .filter(|(_, s)| {
                            s.loading
                                .as_ref()
                                .is_some_and(|l| l.loaded.binding.actor == *actor)
                        })
                        .map(|(key, _)| *key)
                        .collect();
                    self.shard.fanout = Some(Fanout {
                        sequence: event.sequence,
                        recipients,
                        bytes: chat_bytes(text, *chat)?,
                    });
                }
                ShardEffect::Broadcast { text } => {
                    let recipients = self
                        .sessions
                        .iter()
                        .filter(|(_, s)| {
                            !s.terminated
                                && s.loading
                                    .as_ref()
                                    .is_some_and(|l| self.players.entered(l.loaded.binding.actor))
                        })
                        .map(|(key, _)| *key)
                        .collect();
                    self.shard.fanout = Some(Fanout {
                        sequence: event.sequence,
                        recipients,
                        bytes: chat_bytes(text, ShardChat::WorldBroadcast)?,
                    });
                }
            }
        }
        Ok(())
    }
    fn poll_shard_drain(&mut self) -> Result<(), String> {
        let Some(request) = self.shard.control.peek_drain() else {
            return Ok(());
        };
        let ready = match request.kind {
            ShardDrainKind::BootOrdinaryPlayers => {
                for (key, session) in self
                    .sessions
                    .iter_mut()
                    .filter(|(_, s)| {
                        !s.terminated
                            && s.loading.is_some()
                            && request.kind.includes_player(s.account.access_level)
                    })
                    .take(self.limits.work_per_poll)
                {
                    if session.terminated {
                        continue;
                    }
                    if self.network_output.len() >= self.limits.messages {
                        break;
                    }
                    let bytes = bace_wire::AccountControl::Boot {
                        reason: request.kind.boot_message().map(|v| v.0),
                    }
                    .encode()
                    .map_err(|e| e.to_string())?;
                    self.network_output
                        .push_back(NetworkCommand::TerminateAfterFlush {
                            key: *key,
                            queue: 9,
                            bytes,
                        });
                    session.terminated = true;
                    session.closing = true;
                }
                // Session removal occurs only after actual detach, saves, Offline
                // lease acknowledgment and network drain; never a timeout receipt.
                !self.sessions.values().any(|s| {
                    s.loading.is_some() && request.kind.includes_player(s.account.access_level)
                })
            }
            ShardDrainKind::ShutdownPlayers => {
                self.quiesce(self.last_elapsed)?;
                self.sessions.is_empty()
                    && !self.online_saves.requires_drain()
                    && !self.players.requires_drain()
            }
            ShardDrainKind::DisconnectSessions => {
                self.sessions.is_empty()
                    && self.auth_inflight.is_empty()
                    && self.pending_auth.is_none()
                    && self.network_output.is_empty()
            }
            ShardDrainKind::UnloadRegions => {
                self.regions_quiesced
                    && self.world_job.is_none()
                    && self.world.as_ref().is_some_and(|w| {
                        !w.regions.has_pending()
                            && !w.generators.has_pending()
                            && w.events.is_empty()
                    })
            }
            // Final simulation, allegiance, worker and database-world-lease
            // recovery belongs to the retained shutdown owner. No synthetic zero.
            ShardDrainKind::StopWorld => false,
        };
        if ready {
            self.shard
                .control
                .confirm_drain(ShardDrainReceipt {
                    request,
                    remaining: 0,
                    unsaved: 0,
                    pending_operations: 0,
                })
                .map_err(|e| format!("shard drain: {e:?}"))?;
        }
        Ok(())
    }
}
fn chat_bytes(text: &str, chat: ShardChat) -> Result<Vec<u8>, String> {
    if text.len() > 16_384 {
        return Err("shard text limit".into());
    }
    bace_wire::ChatMessage::System {
        text,
        chat_type: match chat {
            ShardChat::Broadcast => 0,
            ShardChat::WorldBroadcast => 0x14,
        },
    }
    .encode()
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
