//! Reliable launch observation precedes scripts and retirement, even when a
//! short flight has already disappeared from the live World by adapter polling.
use super::*;
use bace_simulation::MagicEvent;
use bace_types::EntityId;
pub(super) struct ProjectilePresentation {
    prepared: crate::visibility_assets::PreparedVisibilityObject,
    intensity: f32,
    launch_published: bool,
}
impl GameRuntime {
    pub(super) fn poll_magic_observers(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Some(work) = self.magic.observers.front().cloned() else {
                break;
            };
            if !self.project_magic_observer(&work.event)? {
                break;
            }
            self.acknowledge_magic_observers(work.sequence)?;
        }
        Ok(())
    }
    fn project_magic_observer(&mut self, event: &MagicEvent) -> Result<bool, String> {
        match event {
            MagicEvent::ProjectileCreated {
                actor,
                template,
                launch,
                effect_intensity,
                ..
            } => {
                let launch = launch
                    .as_ref()
                    .ok_or("production magic launch has no accepted audience")?;
                if !self.magic.projectiles.contains_key(actor) {
                    if self.magic.projectiles.len() >= 4096 {
                        return Ok(false);
                    }
                    let template = self
                        .assets
                        .projectile_visibility
                        .get(template)
                        .ok_or("spell projectile cold visibility template absent")?;
                    let prepared = template.instantiate(*actor, launch.tick.max(1), 1)?;
                    self.magic.projectiles.insert(
                        *actor,
                        ProjectilePresentation {
                            prepared,
                            intensity: *effect_intensity,
                            launch_published: false,
                        },
                    );
                }
                let state = self
                    .magic
                    .projectiles
                    .get_mut(actor)
                    .ok_or("spell projectile presentation missing")?;
                if !state.launch_published {
                    state
                        .prepared
                        .register(&mut self.visibility.service, launch.tick)
                        .map_err(|e| format!("spell projectile registration retained: {e:?}"))?;
                    if !self
                        .visibility
                        .service
                        .publish_projectile_launch(launch.clone(), &mut self.players)
                        .map_err(|e| format!("spell launch retained: {e:?}"))?
                    {
                        return Ok(false);
                    }
                    state.launch_published = true;
                }
                self.magic_public_effect(
                    *actor,
                    bace_wire::CombatEffect::Script {
                        object_id: actor.0,
                        script_id: 4,
                        speed: *effect_intensity,
                    },
                )
            }
            MagicEvent::ProjectileExploded { actor, tick, .. } => {
                let state = self
                    .magic
                    .projectiles
                    .get_mut(actor)
                    .ok_or("spell explosion without accepted creation")?;
                if state.prepared.revision == 1 {
                    let mut prepared = state.prepared.clone();
                    prepared.revision = 2;
                    Arc::make_mut(&mut prepared.description).physics.state = 0x128374;
                    prepared
                        .register(&mut self.visibility.service, *tick)
                        .map_err(|e| format!("spell explosion state retained: {e:?}"))?;
                    state.prepared = prepared;
                }
                let intensity = state.intensity;
                self.magic_public_effect(
                    *actor,
                    bace_wire::CombatEffect::Script {
                        object_id: actor.0,
                        script_id: 5,
                        speed: intensity,
                    },
                )
            }
            MagicEvent::ProjectileRemoved { actor, tick } => {
                if self.pending_reward_observers().is_some() || self.observer_output.has_pending() {
                    return Ok(false);
                }
                self.visibility
                    .service
                    .retire_object(*actor, *tick)
                    .map_err(|e| format!("spell projectile retirement retained: {e:?}"))?;
                self.magic.projectiles.remove(actor);
                Ok(true)
            }
            MagicEvent::TargetRejected {
                target,
                reason: CastRejection::Resisted,
                notice,
                ..
            } => {
                let mut private = Vec::with_capacity(2);
                if let Some(notice) = notice {
                    for (binding, text) in [
                        (
                            notice.source,
                            format!("{} resists your spell", notice.target_name),
                        ),
                        (
                            notice.target,
                            format!("You resist the spell cast by {}", notice.source_name),
                        ),
                    ] {
                        let Some(binding) = binding else {
                            continue;
                        };
                        let Some(replica) = self.players.replication(binding.actor) else {
                            continue;
                        };
                        if replica.binding != binding
                            || self
                                .sessions
                                .get(&replica.key)
                                .is_none_or(|s| s.disconnected)
                        {
                            continue;
                        }
                        let bytes = bace_wire::ChatMessage::System {
                            text: &text,
                            chat_type: 7,
                        }
                        .encode()
                        .map_err(|e| e.to_string())?;
                        if bytes.len() > self.limits.message_bytes {
                            return Err("resistance text bound".into());
                        }
                        private.push((replica.key, bytes));
                    }
                }
                let sound = bace_wire::CombatEffect::Sound {
                    object_id: target.0,
                    sound_id: 0x91,
                    volume: 1.,
                }
                .encode(1024, self.limits.message_bytes)
                .map_err(|e| e.to_string())?;
                if self.network_output.len() + private.len() > self.limits.messages
                    || !self.observer_room(1, sound.len())
                {
                    return Ok(false);
                }
                self.retain_observer_messages(vec![(
                    *target,
                    vec![bace_replication::ReplicationMessage {
                        queue: 10,
                        bytes: sound,
                    }],
                )])?;
                for (key, bytes) in private {
                    self.network_output.push_back(NetworkCommand::SendBatch {
                        key,
                        queue: 9,
                        messages: vec![bytes],
                    });
                }
                Ok(true)
            }
            MagicEvent::TargetRejected { .. } => Ok(true),
            _ => Err("non-observer magic event in observer queue".into()),
        }
    }
    fn magic_public_effect(
        &mut self,
        actor: EntityId,
        effect: bace_wire::CombatEffect<'_>,
    ) -> Result<bool, String> {
        let bytes = effect
            .encode(1024, self.limits.message_bytes)
            .map_err(|e| e.to_string())?;
        if !self.observer_room(1, bytes.len()) {
            return Ok(false);
        }
        self.retain_observer_messages(vec![(
            actor,
            vec![bace_replication::ReplicationMessage { queue: 10, bytes }],
        )])?;
        Ok(true)
    }
}
