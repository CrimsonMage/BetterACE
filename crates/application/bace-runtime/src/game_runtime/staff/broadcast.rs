//! Accepted broadcast audience, exact fanout and retained source logging obligation.
use super::*;
use bace_gameplay_api::CharacterBinding;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffBroadcastRecord {
    pub token: u64,
    pub context: ActionContext,
    pub sender: String,
    pub text: String,
}
pub(super) struct BroadcastFanout {
    recipients: VecDeque<CharacterBinding>,
    bytes: Vec<u8>,
}
impl GameRuntime {
    /// Attach the existing bounded host log worker. Neither disk I/O nor a second
    /// worker is created on this event-pump path.
    pub fn attach_staff_broadcast_log(
        &mut self,
        sink: Arc<bace_observability::LogStore>,
    ) -> Result<(), String> {
        if self.staff.broadcast_sink.is_some() {
            return Err("staff log sink already attached".into());
        }
        self.staff.broadcast_sink = Some(sink);
        Ok(())
    }
    pub(in crate::game_runtime::staff) fn poll_staff_broadcast_log(
        &mut self,
    ) -> Result<(), String> {
        let Some(sink) = &self.staff.broadcast_sink else {
            return Ok(());
        };
        let Some(record) = self.staff.broadcast_records.front() else {
            return Ok(());
        };
        let unix_millis = self
            .clock
            .unix_millis
            .checked_add(
                u64::try_from(self.last_elapsed.as_millis())
                    .map_err(|_| "staff log clock overflow")?,
            )
            .ok_or("staff log clock overflow")?;
        match sink.try_record_exact(bace_observability::LogRecord {
            sequence: 0,
            unix_millis,
            level: "INFO".into(),
            event: "chat.global".into(),
            message: format!(
                "[CHAT][GLOBAL] {} issued a world broadcast, \"{}\"",
                record.sender, record.text
            ),
        }) {
            Ok(()) => {
                self.staff.broadcast_records.pop_front();
            }
            Err(error) => match error.kind {
                bace_observability::ExactLogErrorKind::Full => {}
                kind => return Err(format!("retained staff broadcast log rejected: {kind:?}")),
            },
        }
        Ok(())
    }
    /// Pinned PropertyManager chat_log_global defaults false. Policy is immutable
    /// while an accepted broadcast or its logging obligation is outstanding.
    pub fn configure_staff_broadcast_logging(&mut self, enabled: bool) -> Result<(), String> {
        if self.staff.has_pending() {
            return Err("staff work is outstanding".into());
        }
        self.staff.broadcast_logging = enabled;
        Ok(())
    }
    pub fn peek_staff_broadcast_record(&self) -> Option<&StaffBroadcastRecord> {
        self.staff.broadcast_records.front()
    }
    /// Host logging must accept the exact record before acknowledging it.
    pub fn acknowledge_staff_broadcast_record(&mut self, record: &StaffBroadcastRecord) -> bool {
        if self.staff.broadcast_records.front() == Some(record) {
            self.staff.broadcast_records.pop_front();
            true
        } else {
            false
        }
    }
    pub(in crate::game_runtime::staff) fn retain_staff_broadcast(
        &mut self,
        event: &StaffEvent,
    ) -> Result<bool, String> {
        let StaffEvent::Broadcast {
            token,
            context,
            sender,
            recipients,
            text,
        } = event
        else {
            return Err("staff broadcast event shape".into());
        };
        if self.staff.broadcast.is_some()
            || (self.staff.broadcast_logging && self.staff.broadcast_records.len() >= 256)
        {
            return Ok(false);
        }
        let bytes = bace_replication::project_staff_broadcast(text, self.limits.message_bytes)
            .map_err(|e| format!("staff broadcast projection: {e:?}"))?;
        if self.staff.broadcast_logging {
            self.staff
                .broadcast_records
                .push_back(StaffBroadcastRecord {
                    token: *token,
                    context: *context,
                    sender: sender.clone(),
                    text: text.clone(),
                });
        }
        self.staff.broadcast = Some(BroadcastFanout {
            recipients: recipients.clone().into(),
            bytes,
        });
        Ok(true)
    }
    pub(in crate::game_runtime::staff) fn poll_staff_broadcast(&mut self) -> Result<bool, String> {
        let Some(pending) = &mut self.staff.broadcast else {
            return Ok(false);
        };
        while let Some(binding) = pending.recipients.pop_front() {
            let Some(recipient) = self
                .players
                .replication(binding.actor)
                .filter(|p| p.binding == binding)
            else {
                continue;
            };
            self.staff.output.push_back(NetworkCommand::Send {
                key: recipient.key,
                queue: 9,
                bytes: pending.bytes.clone(),
            });
            if pending.recipients.is_empty() {
                self.staff.broadcast = None;
            }
            return Ok(true);
        }
        self.staff.broadcast = None;
        Ok(false)
    }
}
