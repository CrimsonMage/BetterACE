use super::*;
use bace_simulation::{Command, GeneratorAction, GeneratorCommand, GeneratorCommandOutcome};
pub(super) struct Owners {
    pub regions: RegionService,
    pub generators: GeneratorService,
    pub pending: Option<GeneratorCommandOutcome>,
    pub events: VecDeque<bace_simulation::GeneratorWorldEvent>,
}
impl GameRuntime {
    pub(super) fn poll_world(&mut self, elapsed: Duration, unix: u64) -> Result<(), String> {
        if let Some((owners, result)) = ready(&mut self.world_job) {
            self.world = Some(owners);
            self.poll_binding_notices()?;
            return result;
        }
        if self.draining
            && !self.regions_quiesced
            && self.regions_quiesce.is_none()
            && self.sessions.is_empty()
            && self.preparation.pending() == 0
            && self.login_job.is_none()
            && self.login_queue.is_empty()
            && self.request_regions.is_empty()
        {
            let correlation = self.token()?;
            if self
                .simulation
                .input()
                .try_submit(Command::Generator(GeneratorCommand {
                    correlation,
                    action: GeneratorAction::QuiesceRegions,
                }))
                .is_ok()
            {
                self.regions_quiesce = Some(correlation);
            }
        }
        let clock = crate::region_service::RegionClock {
            unix_seconds: i64::try_from(unix / 1000).map_err(|_| "region time overflow")?,
            tick: u64::try_from(elapsed.as_nanos() * 30 / 1_000_000_000)
                .map_err(|_| "region tick overflow")?,
            portal_seconds: self.clock.portal_origin + elapsed.as_secs_f64(),
        };
        let Some(mut world) = self.world.take() else {
            return Ok(());
        };
        if let Some(outcome) = world.pending.take() {
            if self.regions_quiesce == Some(outcome.correlation) {
                match outcome.result {
                    Ok(()) => {
                        self.regions_quiesced = true;
                        self.regions_quiesce = None;
                    }
                    Err(
                        bace_simulation::GeneratorServiceError::Busy
                        | bace_simulation::GeneratorServiceError::Capacity,
                    ) => {
                        self.regions_quiesce = None;
                    }
                    Err(error) => {
                        world.pending = Some(outcome);
                        self.world = Some(world);
                        return Err(format!("region drain rejected; owner retained: {error:?}"));
                    }
                }
            } else if self.npc_region_outcome(outcome.correlation, outcome.result) {
                // An NPC service has its own exact region request owner and no
                // authenticated client binding to insert into request_regions.
            } else if let Some((key, block)) = self.request_regions.remove(&outcome.correlation) {
                if !self.recall_region_outcome(outcome.correlation, outcome.result)
                    && !self.magic_region_outcome(outcome.correlation, outcome.result)
                    && !self.staff_region_outcome(outcome.correlation, outcome.result)
                    && let Err(error) = outcome.result
                {
                    let Some(session) = self.sessions.get_mut(&key) else {
                        self.request_regions
                            .insert(outcome.correlation, (key, block));
                        world.pending = Some(outcome);
                        self.world = Some(world);
                        return Err("region requester missing; owners retained".into());
                    };
                    session.failure = Some(format!("region request: {error:?}"));
                }
            } else {
                world.pending = Some(outcome);
                self.world = Some(world);
                return Err("unrelated generator outcome retained".into());
            }
        }
        if self.draining {
            world.generators.quiesce();
        }
        // Once the simulation confirms region quiescence and both host owners
        // have drained, leave the idle world in this adapter. An unconditional
        // replacement job would keep `world_job` outstanding forever and make
        // the final shutdown proof unreachable.
        if self.draining
            && self.regions_quiesced
            && world.pending.is_none()
            && world.events.is_empty()
            && !world.regions.has_pending()
            && !world.generators.has_pending()
        {
            self.world = Some(world);
            return Ok(());
        }
        // Give lifecycle preparation its immutable region view between I/O turns.
        let worker = self.simulation.clone();
        let saves = self.saves.handle.clone();
        self.world_job = Some(Box::pin(async move {
            let result = async {
                for _ in 0..32 {
                    let Ok(outcome) = worker.generator_outcomes().try_recv() else {
                        break;
                    };
                    if world.regions.owns_generator_outcome(&outcome) {
                        world
                            .regions
                            .accept_generator_outcome(outcome)
                            .map_err(|outcome| {
                                world.pending = Some(*outcome);
                                "region outcome mismatch"
                            })?;
                    } else if world.generators.owns_generator_outcome(&outcome) {
                        world
                            .generators
                            .accept_generator_outcome(outcome)
                            .map_err(|outcome| {
                                world.pending = Some(*outcome);
                                "generator outcome mismatch"
                            })?;
                    } else {
                        world.pending = Some(outcome);
                        break;
                    }
                }
                world.regions.poll(&worker, &saves, clock).await?;
                world
                    .generators
                    .poll(&worker, &mut world.regions, &saves)
                    .await?;
                for _ in 0..32 {
                    if world.events.len() >= 256 {
                        break;
                    }
                    let Ok(event) = worker.generator_events().try_recv() else {
                        break;
                    };
                    let accepted = world
                        .generators
                        .observe_generator_event(&event, &mut world.regions);
                    world.events.push_back(event);
                    accepted?;
                }
                if let Some(error) = world.generators.failure() {
                    return Err(format!("generator service: {error}"));
                }
                if let Some((key, error)) = world.generators.blocked().next() {
                    return Err(format!("generator {key:?} held: {error}"));
                }
                if let Some((block, error)) = world.regions.blocked().next() {
                    return Err(format!("region {block:04x} held: {error}"));
                }
                Ok(())
            }
            .await;
            (world, result)
        }));
        Ok(())
    }
    pub(super) fn prepare_region_for_player(&mut self, key: SessionKey) -> Result<(), String> {
        let loading = self.sessions[&key]
            .loading
            .as_ref()
            .ok_or("region player missing")?;
        let block = lifecycle::landblock(&loading.loaded)?;
        if self
            .world
            .as_ref()
            .is_some_and(|w| w.regions.prepared_region(block).is_some())
        {
            self.sessions
                .get_mut(&key)
                .expect("session")
                .loading
                .as_mut()
                .expect("loading")
                .phase = lifecycle::Phase::Friends;
        } else if !loading.region_requested {
            let token = self.token()?;
            if self
                .simulation
                .input()
                .try_submit(Command::Generator(GeneratorCommand {
                    correlation: token,
                    action: GeneratorAction::RequestRegion {
                        landblock: block,
                        permanent: false,
                    },
                }))
                .is_ok()
            {
                self.request_regions.insert(token, (key, block));
                self.sessions
                    .get_mut(&key)
                    .expect("session")
                    .loading
                    .as_mut()
                    .expect("loading")
                    .region_requested = true;
            }
        }
        Ok(())
    }
}

impl GameRuntime {
    fn poll_binding_notices(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Some(notice) = self
                .world
                .as_ref()
                .and_then(|world| world.regions.binding_notice())
            else {
                break;
            };
            let (object, kind, register) = notice;
            if register {
                self.register_binding_presentation(object, kind)?;
            } else if let Err(error) = self.unregister_binding_presentation(object) {
                if error == "binding action still pending" {
                    break;
                }
                return Err(error);
            }
            self.world
                .as_mut()
                .expect("binding notice world owner")
                .regions
                .acknowledge_binding_notice(notice)?;
        }
        Ok(())
    }
}

impl GameRuntime {
    pub(super) fn poll_generator_publications(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Some(event) = self
                .world
                .as_ref()
                .and_then(|world| world.events.front())
                .cloned()
            else {
                break;
            };
            match event {
                bace_simulation::GeneratorWorldEvent::Spawned { key, entity, .. } => {
                    let Some(publication) = self
                        .world
                        .as_ref()
                        .and_then(|world| world.generators.publication(entity))
                        .cloned()
                    else {
                        // The accepted event can precede its cold host completion.
                        break;
                    };
                    if publication.key != key || publication.entity != entity {
                        return Err("generated visibility publication key mismatch".into());
                    }
                    if let Some(registration) = publication.npc.clone() {
                        let identity = registration.script_identity();
                        self.enqueue_generated_npc_registration(registration)
                            .map_err(|_| "generated NPC source queue retained".to_owned())?;
                        if !self.npc_source_ready(entity, identity) {
                            break;
                        }
                    }
                    let tick = u64::try_from(
                        self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000,
                    )
                    .map_err(|_| "generated visibility tick overflow")?;
                    publication
                        .visibility
                        .register(&mut self.visibility.service, tick)
                        .map_err(|error| format!("generated visibility retained: {error:?}"))?;
                    self.world
                        .as_mut()
                        .expect("publication world owner")
                        .generators
                        .acknowledge_publication(key, entity)?;
                    self.world
                        .as_mut()
                        .expect("publication world owner")
                        .events
                        .pop_front();
                }
                bace_simulation::GeneratorWorldEvent::Lifecycle(effect) => {
                    use bace_gameplay_api::GeneratorLifecycleEffect as E;
                    let id = match effect {
                        E::DestroyMember { member, .. } | E::KillMember { member, .. } => {
                            Some(member.entity)
                        }
                        E::DetachMember { entity, .. } => Some(entity),
                        E::DestroySelf(entity) => Some(entity.entity),
                        E::SuppressedInitial { .. } | E::Invalidated { .. } => None,
                    };
                    if let Some(id) = id {
                        let tick = u64::try_from(
                            self.last_elapsed.as_nanos().saturating_mul(30) / 1_000_000_000,
                        )
                        .map_err(|_| "generated retirement tick overflow")?;
                        self.visibility.retirements.insert(id, tick);
                    }
                    self.world
                        .as_mut()
                        .expect("publication world owner")
                        .events
                        .pop_front();
                }
                bace_simulation::GeneratorWorldEvent::Blocked { .. } => break,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    use bace_gameplay_api::generators::{
        GeneratorIdentity, GeneratorLifecycleEffect, GeneratorLocation, GeneratorSpawnKey,
        GeneratorSpawnMember,
    };
    use bace_replication::Sequences;
    use bace_types::EntityId;
    use bace_wire::{
        ObjectDescription, ObjectGameData, ObjectGameOptions, ObjectModel, PhysicsDescription,
        PhysicsOptions,
    };

    #[tokio::test]
    async fn spawned_publication_waits_for_cold_source_and_retires_after_registration() {
        let (_cluster, _directory, mut runtime) =
            crate::game_runtime::tests::fixture::fixture().await;
        let entity = EntityId(0x8000_0001);
        let generator = GeneratorIdentity {
            entity: EntityId(0x7000_0001),
            incarnation: 1,
            content_revision: runtime.bootstrap.pack.generation.revision(),
            random_identity: [7; 16],
        };
        let key = GeneratorSpawnKey {
            generator,
            profile_id: 1,
            occurrence: 1,
        };
        let spawn = bace_simulation::GeneratorWorldEvent::Spawned {
            birth: None,
            key,
            entity,
            template: 1,
            location: GeneratorLocation {
                cell: 0x0101_0001,
                origin: [0.; 3],
                rotation: [0., 0., 0., 1.],
            },
        };
        runtime
            .world
            .as_mut()
            .unwrap()
            .events
            .push_back(spawn.clone());
        runtime.poll_generator_publications().unwrap();
        assert_eq!(runtime.world.as_ref().unwrap().events.front(), Some(&spawn));
        assert_eq!(
            runtime.visibility.service.registered_object_name(entity),
            None
        );

        let description = Arc::new(ObjectDescription {
            object_id: entity.0,
            model: ObjectModel::default(),
            physics: PhysicsDescription {
                state: 0,
                options: PhysicsOptions::default(),
                sequences: bace_replication::physics_sequences(&Sequences::new(10).unwrap()),
            },
            game: ObjectGameData {
                name: "Generated object".into(),
                class_id: 1,
                icon_id: 0x0600_0001,
                item_type: 16,
                description_flags: 0,
                options: ObjectGameOptions::default(),
            },
        });
        runtime
            .world
            .as_mut()
            .unwrap()
            .generators
            .retain_test_publication(crate::generator_service::PreparedGeneratorPublication {
                key,
                entity,
                visibility: crate::visibility_assets::PreparedVisibilityObject {
                    incarnation: 1,
                    revision: 1,
                    description,
                    children: vec![],
                },
                npc: None,
                retained_bytes: 1,
            });
        runtime.poll_generator_publications().unwrap();
        assert!(runtime.world.as_ref().unwrap().events.is_empty());
        assert!(
            runtime
                .world
                .as_ref()
                .unwrap()
                .generators
                .publication(entity)
                .is_none()
        );
        assert_eq!(
            runtime.visibility.service.registered_object_name(entity),
            Some("Generated object")
        );

        runtime.world.as_mut().unwrap().events.push_back(
            bace_simulation::GeneratorWorldEvent::Lifecycle(
                GeneratorLifecycleEffect::DestroyMember {
                    generator,
                    member: GeneratorSpawnMember {
                        entity,
                        contribution: 1,
                    },
                    recursive: false,
                    include_dead: false,
                    from_unload: false,
                },
            ),
        );
        runtime.poll_generator_publications().unwrap();
        assert!(runtime.world.as_ref().unwrap().events.is_empty());
        runtime.poll_visibility().unwrap();
        assert_eq!(
            runtime.visibility.service.registered_object_name(entity),
            None
        );

        // A separate scripted birth stays owned until the NPC source service
        // reports RecoveryReady; a prepared model alone cannot expose it.
        let scripted_entity = EntityId(entity.0 + 1);
        let scripted_key = GeneratorSpawnKey {
            occurrence: 2,
            ..key
        };
        let mut npc = crate::npc_sources::prepare_registration(
            scripted_entity,
            0x0101,
            1,
            runtime
                .bootstrap
                .pack
                .manifest
                .content_hash(Default::default())
                .unwrap(),
            runtime.bootstrap.pack.generation.clone(),
            1,
        )
        .unwrap();
        npc.admitted = true;
        runtime.world.as_mut().unwrap().events.push_back(
            bace_simulation::GeneratorWorldEvent::Spawned {
                birth: None,
                key: scripted_key,
                entity: scripted_entity,
                template: 1,
                location: GeneratorLocation {
                    cell: 0x0101_0001,
                    origin: [0.; 3],
                    rotation: [0., 0., 0., 1.],
                },
            },
        );
        let description = Arc::new(ObjectDescription {
            object_id: scripted_entity.0,
            model: ObjectModel::default(),
            physics: PhysicsDescription {
                state: 0,
                options: PhysicsOptions::default(),
                sequences: bace_replication::physics_sequences(&Sequences::new(10).unwrap()),
            },
            game: ObjectGameData {
                name: "Held scripted NPC".into(),
                class_id: 1,
                icon_id: 0x0600_0001,
                item_type: 16,
                description_flags: 0,
                options: ObjectGameOptions::default(),
            },
        });
        runtime
            .world
            .as_mut()
            .unwrap()
            .generators
            .retain_test_publication(crate::generator_service::PreparedGeneratorPublication {
                key: scripted_key,
                entity: scripted_entity,
                visibility: crate::visibility_assets::PreparedVisibilityObject {
                    incarnation: 1,
                    revision: 1,
                    description,
                    children: vec![],
                },
                npc: Some(npc),
                retained_bytes: 1,
            });
        runtime.poll_generator_publications().unwrap();
        assert_eq!(runtime.world.as_ref().unwrap().events.len(), 1);
        assert_eq!(
            runtime
                .visibility
                .service
                .registered_object_name(scripted_entity),
            None
        );
    }
}
