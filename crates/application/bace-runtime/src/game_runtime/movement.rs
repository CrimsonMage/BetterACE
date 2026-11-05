//! Untrusted client observations select controls; only a correlated simulation
//! result can spend stamina or request a correction from accepted world state.
use super::*;
use bace_gameplay_api::{
    ActionContext,
    locomotion::{LocomotionCommand, LocomotionOutcome, LocomotionRejection},
};
use bace_replication::SequenceKind as K;
use bace_session::{DispatchError, LocomotionObservations, SessionState};
#[cfg(test)]
mod tests;
struct Pending {
    context: ActionContext,
    correlation: u64,
    observations: LocomotionObservations,
    result: Option<Arc<LocomotionOutcome>>,
}
pub(super) struct MovementRuntime {
    pending: BTreeMap<SessionKey, Pending>,
    unexpected: Option<Arc<LocomotionOutcome>>,
    diagnostic_counts: BTreeMap<(SessionKey, u8), u32>,
    last_state: BTreeMap<SessionKey, bace_gameplay_api::locomotion::RawLocomotionState>,
}
pub(super) enum MovementIngress {
    Accepted,
    Blocked,
    Unsupported,
}
impl MovementRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            unexpected: None,
            diagnostic_counts: BTreeMap::new(),
            last_state: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || self.unexpected.is_some()
    }
    fn diagnose(&mut self, key: SessionKey, kind: u8) -> bool {
        if self.diagnostic_counts.len() >= 512
            && !self.diagnostic_counts.contains_key(&(key, kind))
            && let Some(oldest) = self.diagnostic_counts.keys().next().copied()
        {
            self.diagnostic_counts.remove(&oldest);
        }
        let count = self.diagnostic_counts.entry((key, kind)).or_default();
        *count = count.saturating_add(1);
        *count <= 3 || (*count).is_multiple_of(128)
    }
}
fn rejection_diagnostic_kind(reason: LocomotionRejection) -> u8 {
    use LocomotionRejection as R;
    match reason {
        R::NotBound => 10,
        R::Stale => 11,
        R::Busy => 12,
        R::Invalid => 13,
        R::MissingAssets => 14,
        R::TooTired => 15,
        R::Capacity => 16,
    }
}
impl GameRuntime {
    pub(super) fn handle_priority_cast_cancel(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<MovementIngress, String> {
        let envelope = match bace_wire::GameActionEnvelope::decode(
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value,
            Err(_) => return Ok(MovementIngress::Unsupported),
        };
        if envelope.action != bace_wire::opcode::GameActionType::CancelAttack {
            return Ok(MovementIngress::Unsupported);
        }
        let Some(binding) = self.players.binding_for(key) else {
            return Ok(MovementIngress::Accepted);
        };
        if !self.players.entered(binding.actor) {
            return Ok(MovementIngress::Blocked);
        }
        let result = self.handle_player_magic_cancel(ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        })?;
        Ok(match result {
            magic::MagicIngress::Accepted => MovementIngress::Accepted,
            magic::MagicIngress::Blocked => MovementIngress::Blocked,
            magic::MagicIngress::Unsupported => MovementIngress::Unsupported,
        })
    }
    pub(super) fn movement_pending(&self, key: SessionKey) -> bool {
        self.movement.pending.contains_key(&key)
    }
    pub(super) fn handle_movement_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<MovementIngress, String> {
        let envelope = match bace_wire::GameActionEnvelope::decode(
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value,
            Err(_) => return Ok(MovementIngress::Unsupported),
        };
        use bace_wire::opcode::GameActionType as A;
        if !matches!(
            envelope.action,
            A::MoveToState | A::Jump | A::JumpNonAutonomous | A::AutonomousPosition
        ) {
            return Ok(MovementIngress::Unsupported);
        }
        if self.movement.pending.contains_key(&key) {
            if self.movement.diagnose(key, 3) {
                self.diagnostic(
                    "info",
                    "game.movement_deferred",
                    &format!(
                        "session {} generation {}: action {:#06x}, message sequence {}, prior motion awaiting receipt",
                        key.id, key.generation, envelope.action.0, message.sequence
                    ),
                );
            }
            return Ok(MovementIngress::Blocked);
        }
        let Some(binding) = self.players.binding_for(key) else {
            if self.movement.diagnose(key, 4) {
                self.diagnostic(
                    "warn",
                    "game.movement_unbound",
                    &format!(
                        "session {} generation {}: action {:#06x}, message sequence {}",
                        key.id, key.generation, envelope.action.0, message.sequence
                    ),
                );
            }
            return Ok(MovementIngress::Accepted);
        };
        if !self.players.entered(binding.actor) {
            if self.movement.diagnose(key, 5) {
                self.diagnostic(
                    "info",
                    "game.movement_before_entry",
                    &format!(
                        "session {} generation {}: action {:#06x}, message sequence {}",
                        key.id, key.generation, envelope.action.0, message.sequence
                    ),
                );
            }
            return Ok(MovementIngress::Blocked);
        }
        self.visibility
            .service
            .bind(key, binding)
            .map_err(|error| format!("movement observer binding: {error:?}"))?;
        let mut input = match bace_session::decode_locomotion(
            SessionState::WorldConnected,
            ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: 0,
            },
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(input) => input,
            Err(DispatchError::Wire(error)) => {
                if self.movement.diagnose(key, 2) {
                    self.diagnostic(
                        "error",
                        "game.movement_decode_failed",
                        &format!(
                            "session {} generation {}: action {:#06x}, bytes {}, error {error:?}",
                            key.id,
                            key.generation,
                            envelope.action.0,
                            message.bytes.len()
                        ),
                    );
                }
                // This packet has not reached the owner. Close only its peer;
                // already accepted work continues through normal durable logout.
                self.sessions
                    .get_mut(&key)
                    .ok_or("movement session disappeared")?
                    .terminated = true;
                return Ok(MovementIngress::Accepted);
            }
            Err(error) => return Err(format!("movement dispatch: {error:?}")),
        };
        input.context.sequence = message.sequence;
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("movement counter owner missing")?;
        let epochs = bace_wire::MovementEpochs {
            instance: replica.properties.current(K::ObjectInstance, 0),
            server_control: replica.properties.current(K::ObjectServerControl, 0),
            teleport: replica.properties.current(K::ObjectTeleport, 0),
            force_position: replica.properties.current(K::ObjectForcePosition, 0),
        };
        if input.epochs.is_some_and(|observed| {
            !bace_session::movement_epochs_compatible(envelope.action, observed, epochs)
        }) {
            if self.movement.diagnose(key, 1) {
                self.diagnostic(
                    "warn",
                    "game.movement_epoch_mismatch",
                    &format!(
                        "session {} generation {}: action {:#06x}, message sequence {}, client {:?}, server {:?}",
                        key.id, key.generation, envelope.action.0, message.sequence, input.epochs, epochs
                    ),
                );
            }
            // Pinned GDLE discards stale teleport/force-position observations
            // without incrementing ForcePosition or issuing another update.
            // The canonical visibility stream retains the accepted state.
            return Ok(MovementIngress::Accepted);
        }
        let correlation = self.token()?;
        let command = bace_simulation::Command::Locomotion(LocomotionCommand {
            correlation,
            context: input.context,
            // An admissible future client stamp cannot choose the authoritative
            // Body epoch. The simulation owner uses only its canonical value.
            teleport: epochs.teleport,
            request: input.request,
        });
        match self.simulation.input().try_submit(command) {
            Ok(()) => {
                let changed =
                    if let bace_gameplay_api::locomotion::LocomotionRequest::State(state) =
                        input.request
                    {
                        if self.movement.last_state.len() >= 512
                            && !self.movement.last_state.contains_key(&key)
                            && let Some(oldest) = self.movement.last_state.keys().next().copied()
                        {
                            self.movement.last_state.remove(&oldest);
                        }
                        self.movement.last_state.insert(key, state) != Some(state)
                    } else {
                        false
                    };
                if changed || self.movement.diagnose(key, 0) {
                    self.diagnostic(
                        "info",
                        "game.movement_submitted",
                        &format!(
                            "session {} generation {}: action {:#06x}, message sequence {}, client epochs {:?}, server epochs {:?}, request {:?}",
                            key.id, key.generation, envelope.action.0, message.sequence, input.epochs, epochs, input.request
                        ),
                    );
                }
                self.movement.pending.insert(
                    key,
                    Pending {
                        context: input.context,
                        correlation,
                        observations: input.observations,
                        result: None,
                    },
                );
                Ok(MovementIngress::Accepted)
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(MovementIngress::Blocked),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("movement owner closed; original input retained".into())
            }
        }
    }
    pub(super) fn poll_movement(&mut self) -> Result<(), String> {
        if self.movement.unexpected.is_some() {
            return Err("unrelated movement outcome retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.locomotion_outcomes().try_recv() else {
                break;
            };
            let Some(pending) = self.movement.pending.values_mut().find(|p| {
                p.correlation == outcome.correlation
                    && p.context == outcome.context
                    && p.result.is_none()
            }) else {
                self.movement.unexpected = Some(outcome);
                return Err("movement result fence mismatch".into());
            };
            pending.result = Some(outcome);
        }
        let keys: Vec<_> = self
            .movement
            .pending
            .iter()
            .filter(|(_, p)| p.result.is_some())
            .take(self.limits.work_per_poll)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            let pending = &self.movement.pending[&key];
            let result = pending.result.as_ref().expect("selected result");
            let receipt_diagnostic = (
                result.context.sequence,
                result.tick,
                result.result.as_ref().err().copied(),
            );
            let connected = self
                .sessions
                .get(&key)
                .is_some_and(|s| !s.disconnected && !s.terminated);
            let mut correction_detail = None;
            // A closed peer needs no private correction. Its accepted motion
            // receipt must clear even when the outbound queue is saturated,
            // or durable logout waits forever on movement_pending.
            if connected && self.network_output.len() >= self.limits.messages {
                break;
            }
            if connected {
                let correction = match &result.result {
                    Ok(accepted) => needs_forced_position(pending.observations, &accepted.view),
                    Err(_) => true,
                };
                if correction {
                    correction_detail = Some(match &result.result {
                        Ok(accepted) => format!(
                            "accepted view cell {:#010x} position {:?}, client observation {:?}",
                            accepted.view.cell, accepted.view.position, pending.observations
                        ),
                        Err(reason) => format!("rejected by simulation: {reason:?}"),
                    });
                    self.visibility
                        .service
                        .request_self_correction(key)
                        .map_err(|error| format!("movement correction retained: {error:?}"))?;
                }
                let replica = self
                    .players
                    .replication(result.context.actor)
                    .ok_or("movement receipt lost sequence owner")?;
                let mut projected = (**result).clone();
                if let Ok(accepted) = &mut projected.result
                    && replica.vital_revisions[1].is_some_and(|r| r >= accepted.vital_revision)
                {
                    accepted.stamina = None;
                }
                let batch = replica
                    .events
                    .project_locomotion_outcome(
                        replica.binding,
                        &projected,
                        &mut replica.properties,
                        bace_replication::BatchLimits {
                            max_messages: 2,
                            max_bytes: 256,
                            max_message_bytes: 128,
                            max_string_bytes: 0,
                        },
                    )
                    .map_err(|error| format!("movement private projection: {error:?}"))?;
                if let Ok(accepted) = &projected.result
                    && accepted.stamina.is_some()
                {
                    replica.vital_revisions[1] = Some(accepted.vital_revision);
                }
                if !batch.messages.is_empty() {
                    self.network_output
                        .push_back(NetworkCommand::SendOrderedBatch {
                            key,
                            messages: batch
                                .messages
                                .into_iter()
                                .map(|m| (m.queue, m.bytes))
                                .collect(),
                        });
                }
            }
            self.movement.pending.remove(&key);
            if let Some(detail) = correction_detail
                && self.movement.diagnose(key, 6)
            {
                self.diagnostic(
                    "info",
                    "game.movement_correction_requested",
                    &format!(
                        "session {} generation {}: message sequence {}, {detail}",
                        key.id, key.generation, receipt_diagnostic.0
                    ),
                );
            }
            if let Some(reason) = receipt_diagnostic.2
                && self
                    .movement
                    .diagnose(key, rejection_diagnostic_kind(reason))
            {
                self.diagnostic(
                    "warn",
                    "game.movement_rejected",
                    &format!(
                        "session {} generation {}: message sequence {}, simulation tick {}, reason {reason:?}",
                        key.id, key.generation, receipt_diagnostic.0, receipt_diagnostic.1
                    ),
                );
            }
        }
        Ok(())
    }
}
fn needs_forced_position(
    observation: LocomotionObservations,
    view: &bace_gameplay_api::visibility::AcceptedObjectView,
) -> bool {
    let (position, threshold) = match observation {
        LocomotionObservations::State { position, .. } => (position, 10.0_f32),
        LocomotionObservations::Position { position, .. } => (position, 25.0_f32),
        _ => return false,
    };
    // Pinned GDLE ClientEvents.cpp uses 10m for MoveToState and 25m for
    // AutonomousPosition before a forced correction. Ordinary client prediction,
    // contact and heading differences must not advance ForcePosition every key
    // press. Neither the observation nor this comparison accepts a client pose.
    // A cross-cell distance needs the authored cell transform; regular canonical
    // visibility updates continue while we retain server physics authority.
    position.cell == view.cell
        && position
            .origin
            .into_iter()
            .zip(view.position)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            >= threshold * threshold
}
