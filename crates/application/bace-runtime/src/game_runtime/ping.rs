//! Private PingRequest answer. ACE `GameActionPingRequest` replies with a
//! GameEventPingResponse only after the session is WorldConnected and the
//! player is attached; earlier requests are dropped without a reply and without
//! failing the session. The reply uses the replica's shared event counter and
//! the bounded private UIQueue output, never the socket.
use super::*;
use bace_replication::{BatchLimits, EventSequencer, InventoryProjection, SessionBatch};
use bace_wire::{ObjectCodecLimits, SimpleGameEvent};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

/// Routing result for the ingress cascade. `Dropped` is consumed without output;
/// `Blocked` is retained for a later poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PingIngress {
    Accepted,
    Dropped,
    Blocked,
    Unsupported,
}

/// Pure gating table, separated from runtime state so every row is testable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PingState {
    pub closing: bool,
    pub world_entered: bool,
    pub replication_ready: bool,
    pub output_capacity: bool,
}
pub(super) fn ping_gate(state: PingState) -> PingIngress {
    if state.closing || !state.world_entered {
        PingIngress::Dropped
    } else if !state.replication_ready || !state.output_capacity {
        PingIngress::Blocked
    } else {
        PingIngress::Accepted
    }
}

/// One GameEventPingResponse through the replica's shared event counter. The
/// counter advances only when the whole batch is accepted.
pub(super) fn project_ping_response(
    events: &mut EventSequencer,
    binding: bace_gameplay_api::CharacterBinding,
    max_message_bytes: usize,
) -> Result<SessionBatch, String> {
    let objects = ObjectCodecLimits {
        max_message_bytes,
        max_model_entries: 0,
        max_children: 0,
        max_restrictions: 0,
        max_motion_commands: 0,
        max_string_bytes: 0,
    };
    let limits = BatchLimits {
        max_messages: 1,
        max_bytes: max_message_bytes,
        max_message_bytes,
        max_string_bytes: 0,
    };
    // Simple events use the shared event counter; the object codec is unused here.
    events
        .project_inventory(
            binding,
            &[InventoryProjection::Simple(SimpleGameEvent::PingResponse)],
            &mut BTreeMap::new(),
            objects,
            limits,
        )
        .map_err(|error| format!("ping response projection: {error:?}"))
}

impl GameRuntime {
    pub(super) fn handle_ping_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<PingIngress, String> {
        let Ok(envelope) =
            bace_wire::GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes)
        else {
            return Ok(PingIngress::Unsupported);
        };
        if envelope.action != bace_wire::opcode::GameActionType::PingRequest {
            return Ok(PingIngress::Unsupported);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(PingIngress::Blocked);
        };
        let closing =
            self.draining || session.terminated || session.disconnected || session.closing;
        let account = session.account.id;
        let binding = session
            .loading
            .as_ref()
            .map(|loading| loading.loaded.binding)
            .filter(|binding| self.players.entered(binding.actor));
        let Some(binding) = binding else {
            // Pre-entry requests are dropped, matching ACE's WorldConnected gate.
            return Ok(ping_gate(PingState {
                closing,
                world_entered: false,
                replication_ready: false,
                output_capacity: true,
            }));
        };
        if binding.account != account || binding.session.0 != key.generation {
            return Err("ping authenticated binding mismatch".into());
        }
        let output_capacity = self.network_output.len() < self.limits.messages;
        let Some(replica) = self.players.replication(binding.actor) else {
            return Ok(ping_gate(PingState {
                closing,
                world_entered: true,
                replication_ready: false,
                output_capacity,
            }));
        };
        let replication_ready = replica.key == key && replica.binding == binding;
        if !replication_ready {
            return Err("ping replication binding mismatch".into());
        }
        let gate = ping_gate(PingState {
            closing,
            world_entered: true,
            replication_ready,
            output_capacity,
        });
        if gate != PingIngress::Accepted {
            return Ok(gate);
        }
        let batch = project_ping_response(&mut replica.events, binding, self.limits.message_bytes)?;
        queue_ping(self, key, batch)?;
        Ok(PingIngress::Accepted)
    }
}

fn queue_ping(
    runtime: &mut GameRuntime,
    key: SessionKey,
    batch: SessionBatch,
) -> Result<(), String> {
    if batch.messages.is_empty() {
        return Err("ping response projected no message".into());
    }
    if batch.messages.iter().any(|m| m.queue != 9) {
        return Err("private ping queue mismatch".into());
    }
    runtime.network_output.push_back(NetworkCommand::SendBatch {
        key,
        queue: 9,
        messages: batch.messages.into_iter().map(|m| m.bytes).collect(),
    });
    Ok(())
}
