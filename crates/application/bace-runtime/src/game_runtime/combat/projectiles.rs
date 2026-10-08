//! Immutable projectile appearance and exact lifecycle events. Motion/position
//! continue to come from the single simulation owner's accepted object views.
use super::*;
use crate::visibility_assets::PreparedVisibilityObject;
use bace_simulation::PhysicalCombatEvent as P;
pub(super) struct Projectiles {
    pub(super) blueprints: BTreeMap<EntityId, PreparedVisibilityObject>,
    cold: Option<Job<Result<PreparedVisibilityObject, String>>>,
}
impl Projectiles {
    pub(super) fn new() -> Self {
        Self {
            blueprints: BTreeMap::new(),
            cold: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.blueprints.is_empty() || self.cold.is_some()
    }
}
impl GameRuntime {
    pub(super) fn project_physical_projectile(&mut self, event: &P) -> Result<bool, String> {
        match event {
            P::ProjectileCreated {
                proposal,
                tick,
                launch,
                appearance,
            } => {
                let id = EntityId(proposal.projectile);
                if !self.combat.projectiles.blueprints.contains_key(&id) {
                    if self.combat.projectiles.blueprints.len() >= 4096 {
                        return Ok(false);
                    }
                    if self.combat.projectiles.cold.is_none() {
                        let source = appearance
                            .as_ref()
                            .ok_or("accepted projectile ammunition source missing")?
                            .as_ref()
                            .clone();
                        let manifest = self.bootstrap.assets.clone();
                        let proposal = proposal.clone();
                        self.combat.projectiles.cold = Some(Box::pin(async move {
                            tokio::task::spawn_blocking(move || {
                                resources::cold_blueprint(manifest, source, proposal)
                            })
                            .await
                            .map_err(|e| e.to_string())?
                        }));
                    }
                    if let Some(job) = self.combat.projectiles.cold.as_mut()
                        && let Poll::Ready(result) =
                            job.as_mut().poll(&mut Context::from_waker(Waker::noop()))
                    {
                        self.combat.projectiles.cold = None;
                        self.combat.projectiles.blueprints.insert(id, result?);
                    }
                    if !self.combat.projectiles.blueprints.contains_key(&id) {
                        return Ok(false);
                    }
                }
                self.combat.projectiles.blueprints[&id]
                    .register(&mut self.visibility.service, *tick)
                    .map_err(|e| format!("projectile appearance retained: {e:?}"))?;
                if !self
                    .visibility
                    .service
                    .publish_projectile_launch(launch.clone(), &mut self.players)
                    .map_err(|e| format!("projectile launch admission retained: {e:?}"))?
                {
                    return Ok(false);
                }
                self.physical_effect(
                    id,
                    bace_wire::CombatEffect::Script {
                        object_id: id.0,
                        script_id: 4,
                        speed: 0.,
                    },
                )
            }
            P::ProjectileDestroy { actor, .. } => self.physical_effect(
                *actor,
                bace_wire::CombatEffect::Script {
                    object_id: actor.0,
                    script_id: 89,
                    speed: 1.,
                },
            ),
            P::ProjectileResting { actor, tick } => {
                let source = self
                    .combat
                    .projectiles
                    .blueprints
                    .get_mut(actor)
                    .ok_or("resting projectile source missing")?;
                if source.revision == 1 {
                    source.revision = 2;
                    let description = Arc::make_mut(&mut source.description);
                    description.physics.state = 0x20414;
                    description.physics.options.movement =
                        Some(bace_wire::PhysicsMovement::AnimationFrame(101));
                }
                source
                    .register(&mut self.visibility.service, *tick)
                    .map_err(|e| format!("resting projectile handoff: {e:?}"))?;
                self.physical_effect(
                    *actor,
                    bace_wire::CombatEffect::Sound {
                        object_id: actor.0,
                        sound_id: 0x2f,
                        volume: 1.,
                    },
                )
            }
            P::ProjectileRemoved { actor, tick } => {
                // Older public effects must enter reliable ownership before
                // removal can erase this generation's known audience.
                if self.pending_reward_observers().is_some() || self.observer_output.has_pending() {
                    return Ok(false);
                }
                self.visibility
                    .service
                    .retire_object(*actor, *tick)
                    .map_err(|e| format!("projectile removal retained: {e:?}"))?;
                self.combat.projectiles.blueprints.remove(actor);
                Ok(true)
            }
            _ => Err("non-projectile event in presentation lane".into()),
        }
    }
    fn physical_effect(
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
