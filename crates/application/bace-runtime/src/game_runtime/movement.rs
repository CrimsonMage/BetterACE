//! Untrusted client observations select controls; only a correlated simulation
//! result can spend stamina or request a correction from accepted world state.
use super::*;
use bace_gameplay_api::{
    ActionContext,
    locomotion::{LocomotionCommand, LocomotionOutcome},
};
use bace_replication::SequenceKind as K;
use bace_session::{DispatchError, LocomotionObservations, SessionState};
struct Pending {
    context: ActionContext,
    correlation: u64,
    observations: LocomotionObservations,
    result: Option<Arc<LocomotionOutcome>>,
}
pub(super) struct MovementRuntime {
    pending: BTreeMap<SessionKey, Pending>,
    unexpected: Option<Arc<LocomotionOutcome>>,
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
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || self.unexpected.is_some()
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
            return Ok(MovementIngress::Blocked);
        }
        let Some(binding) = self.players.binding_for(key) else {
            return Ok(MovementIngress::Accepted);
        };
        if !self.players.entered(binding.actor) {
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
            Err(DispatchError::Wire(_)) => {
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
        if input
            .epochs
            .is_some_and(|observed| !bace_session::movement_epochs_match(observed, epochs))
        {
            self.visibility
                .service
                .request_self_correction(key)
                .map_err(|error| format!("movement epoch correction: {error:?}"))?;
            return Ok(MovementIngress::Accepted);
        }
        let correlation = self.token()?;
        let command = bace_simulation::Command::Locomotion(LocomotionCommand {
            correlation,
            context: input.context,
            teleport: input
                .epochs
                .map_or(epochs.teleport, |observed| observed.teleport),
            request: input.request,
        });
        match self.simulation.input().try_submit(command) {
            Ok(()) => {
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
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            let pending = &self.movement.pending[&key];
            let result = pending.result.as_ref().expect("selected result");
            let connected = self
                .sessions
                .get(&key)
                .is_some_and(|s| !s.disconnected && !s.terminated);
            if connected {
                let correction = match &result.result {
                    Ok(accepted) => differs(pending.observations, &accepted.view),
                    Err(_) => true,
                };
                if correction {
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
        }
        Ok(())
    }
}
fn differs(
    observation: LocomotionObservations,
    view: &bace_gameplay_api::visibility::AcceptedObjectView,
) -> bool {
    let (position, contact) = match observation {
        LocomotionObservations::State {
            position, contact, ..
        }
        | LocomotionObservations::Position { position, contact } => (position, contact),
        _ => return false,
    };
    // Reconciliation tolerance is an explicit authority policy, not a promise of
    // retail prediction parity. Neither side of this comparison mutates physics.
    let norm = position.rotation.into_iter().map(|v| v * v).sum::<f32>();
    let half = view.heading_radians * 0.5;
    let aligned = (position.rotation[0] * half.cos() + position.rotation[3] * half.sin()).abs();
    (norm - 1.).abs() > 0.01
        || aligned < 0.9996875
        || position.cell != view.cell
        || contact != view.grounded
        || position
            .origin
            .into_iter()
            .zip(view.position)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            > 0.0625
}
