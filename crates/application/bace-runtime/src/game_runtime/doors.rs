//! Authored door Use ingress and retained output. Door state stays on the
//! simulation owner; this adapter only submits requests and projects receipts.
use super::*;
use bace_gameplay_api::{ActionContext, DoorOutcome, UseDoor};
use bace_simulation::{Command, DoorEvent};
use bace_types::EntityId;
use bace_wire::{GameActionEnvelope, InventoryAction};

pub(super) enum DoorIngress {
    Accepted,
    Blocked,
    Unsupported,
}

#[derive(Default)]
pub(super) struct DoorRuntime {
    outcome: Option<DoorOutcome>,
    event: Option<DoorEvent>,
}

impl GameRuntime {
    pub(in crate::game_runtime) fn handle_door_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<DoorIngress, String> {
        use bace_wire::opcode::{GameActionType as Op, GameMessageOpcode};
        if message
            .bytes
            .get(..4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            != Some(GameMessageOpcode::GameAction.0)
        {
            return Ok(DoorIngress::Unsupported);
        }
        let envelope = GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes)
            .map_err(|error| error.to_string())?;
        if envelope.action != Op::Use {
            return Ok(DoorIngress::Unsupported);
        }
        let decoded = bace_wire::InventoryRequest::decode(
            envelope.action,
            envelope.payload,
            self.limits.message_bytes,
            0,
        )
        .map_err(|error| error.to_string())?;
        let InventoryAction::Use(raw_target) = decoded.action else {
            return Ok(DoorIngress::Unsupported);
        };
        let door = EntityId(raw_target);
        let Some(world) = self.world.as_ref() else {
            return Ok(DoorIngress::Blocked);
        };
        if !world.regions.is_active_door(door) {
            return Ok(DoorIngress::Unsupported);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(DoorIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(DoorIngress::Blocked);
        };
        let binding = loading.loaded.binding;
        if self.draining
            || session.disconnected
            || session.terminated
            || !self.players.entered(binding.actor)
        {
            return Ok(DoorIngress::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("door authenticated session binding".into());
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        match self.simulation.input().try_submit(Command::UseDoor {
            context,
            request: UseDoor { door },
        }) {
            Ok(()) => {
                self.diagnostic(
                    "info",
                    "game.door_use_submitted",
                    &format!("actor {:#010x} door {:#010x}", binding.actor.0, door.0),
                );
                Ok(DoorIngress::Accepted)
            }
            Err(_) => Ok(DoorIngress::Blocked),
        }
    }

    pub(in crate::game_runtime) fn poll_doors(&mut self) -> Result<(), String> {
        if self.doors.event.is_none() {
            self.doors.event = self.simulation.door_events().try_recv().ok();
        }
        if let Some(event) = self.doors.event {
            if let Some(motion) = event.motion {
                if !self.observer_room(1, 4096) {
                    return Ok(());
                }
                let view = bace_wire::MovementDescription {
                    autonomous: false,
                    motion_flags: 0,
                    current_style: event.style,
                    body: bace_wire::MotionBody::State {
                        state: bace_wire::InterpretedMotion {
                            current_style: Some(event.style),
                            forward_command: Some(3),
                            commands: vec![bace_wire::MotionCommandItem {
                                raw_command: if motion.open { 0x000b } else { 0x000c },
                                sequence: motion.animation_sequence,
                                autonomous: false,
                                speed: 1.0,
                            }],
                            ..Default::default()
                        },
                        sticky_object: None,
                    },
                };
                let projected = match self
                    .visibility
                    .service
                    .project_object_motion(event.door, &view)
                {
                    Ok(projected) => projected,
                    Err(crate::visibility_service::VisibilityServiceError::Stale) => {
                        // The region may be admitted before its visibility
                        // source arrives. Retain the exact event until it does;
                        // discard only after that authored region retired.
                        if !self
                            .world
                            .as_ref()
                            .is_none_or(|world| world.regions.is_active_door(event.door))
                        {
                            self.doors.event = None;
                        }
                        return Ok(());
                    }
                    Err(error) => return Err(format!("door motion projection: {error:?}")),
                };
                self.retain_observer_messages(vec![(event.door, vec![projected])])?;
            }
            self.doors.event = None;
        }
        if self.doors.outcome.is_none() {
            self.doors.outcome = self.simulation.door_outcomes().try_recv().ok();
        }
        let Some(outcome) = self.doors.outcome.as_ref() else {
            return Ok(());
        };
        let binding = bace_gameplay_api::CharacterBinding {
            actor: outcome.context.actor,
            account: outcome.context.account,
            session: outcome.context.session,
        };
        let Some(replica) = self.players.replication(binding.actor) else {
            self.doors.outcome = None;
            return Ok(());
        };
        let key = replica.key;
        if self.sessions.get(&key).is_none_or(|session| {
            session.disconnected || session.terminated || replica.binding != binding
        }) {
            self.doors.outcome = None;
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let code = match outcome.result {
            Ok(_) => 0,
            Err(error) => {
                self.diagnostic(
                    "warn",
                    "game.door_use_rejected",
                    &format!("actor {:#010x}: {error:?}", binding.actor.0),
                );
                0x0036
            }
        };
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("door use sequence owner absent")?;
        let batch = replica
            .events
            .project_inventory(
                binding,
                &[bace_replication::InventoryProjection::Simple(
                    bace_wire::SimpleGameEvent::UseDone(code),
                )],
                &mut replica.item_properties,
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: self.limits.message_bytes,
                    max_model_entries: 255,
                    max_children: 255,
                    max_restrictions: 1024,
                    max_motion_commands: 4096,
                    max_string_bytes: 4096,
                },
                bace_replication::BatchLimits {
                    max_messages: self.limits.messages,
                    max_bytes: self.limits.message_bytes,
                    max_message_bytes: self.limits.message_bytes,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("door UseDone: {error:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        self.doors.outcome = None;
        Ok(())
    }
}
