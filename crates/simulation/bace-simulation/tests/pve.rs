use std::sync::Arc;

use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::Actor;
use bace_entity::{Combatant, CombatantProfile};
use bace_gameplay_api::CombatRequest;
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionTarget, SessionId, SkillAdvancement,
};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::{Command, Kernel};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;

fn kernel(commands: usize, characters: usize, outcomes: usize) -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    world
        .register_scene(
            CellId(2),
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=1 {
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
    }
    for id in 1..=1 {
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 20,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: id == 1,
                })
                .unwrap()
                .with_resources(
                    Some(bace_entity::VitalPool {
                        current: 10,
                        maximum: 10,
                    }),
                    Some(bace_entity::VitalPool {
                        current: 10,
                        maximum: 10,
                    }),
                )
                .unwrap(),
            )
            .unwrap();
    }
    let mut kernel = Kernel::with_gameplay_limits(world, commands, characters, outcomes).unwrap();
    kernel.register_character(binding(), character()).unwrap();
    kernel
}

fn character() -> CharacterProgression {
    let table = RankTable::new(&[0, 1, 10, 100]).unwrap();
    CharacterProgression::new(
        &[TraitProgress {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            experience_spent: 0,
            advancement: SkillAdvancement::Inactive,
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        0,
    )
    .unwrap()
}

fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(9),
        account: AccountId(4),
        actor: EntityId(1),
    }
}

fn context(sequence: u32) -> ActionContext {
    let binding = binding();
    ActionContext {
        session: binding.session,
        account: binding.account,
        actor: binding.actor,
        sequence,
    }
}

use bace_simulation::{CharacterRegistrationError, NpcBlueprint, NpcLootEntry, PveError, PveEvent};
fn npc() -> NpcBlueprint {
    NpcBlueprint {
        cell: CellId(1),
        position: Vec3::new(2.0, 0.0, 0.5),
        radius: 0.5,
        capabilities: Capabilities {
            speed: 1.0,
            jump_impulse: 1.0,
        },
        combat: CombatantProfile {
            maximum_health: 20,
            melee_damage: 2,
            melee_range: 2.0,
            attack_duration: 0.1,
            strike_offsets: vec![0.05],
            player: false,
        },
        visual_range: 10.0,
        think_interval: 1,
        corpse_template: 9000,
        xp_override: Some(100),
        loot: vec![
            NpcLootEntry {
                template: 42,
                destination: 8,
                probability: 0.25,
                stack: 1,
            },
            NpcLootEntry {
                template: 43,
                destination: 8,
                probability: 0.75,
                stack: 2,
            },
        ],
        death_animation_ticks: 2,
        respawn_ticks: 3,
        corpse_decay_ticks: 5,
    }
}
fn tick(kernel: &mut Kernel) {
    kernel.step().unwrap();
    while kernel.take_combat_event().is_some() {}
    while kernel.take_combat_outcome().is_some() {}
}
#[test]
fn npc_attacks_dies_then_loot_and_reward_wait_for_confirmed_durability() {
    let mut kernel = kernel(16, 2, 16);
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel.supply_loot_random(&[0.8]).unwrap();
    kernel.supply_spawn_id(EntityId(4)).unwrap();
    tick(&mut kernel);
    tick(&mut kernel);
    assert_eq!(kernel.world().combatant(EntityId(1)).unwrap().health(), 8);
    kernel
        .enqueue(Command::Combat {
            context: context(1),
            request: CombatRequest::ChangeMode(2),
        })
        .unwrap();
    kernel
        .enqueue(Command::Combat {
            context: context(2),
            request: CombatRequest::TargetedMelee {
                target: EntityId(3),
                height: 2,
                power: 0.5,
            },
        })
        .unwrap();
    tick(&mut kernel);
    tick(&mut kernel);
    assert_eq!(kernel.world().combatant(EntityId(3)).unwrap().health(), 0);
    assert_eq!(kernel.pending_deaths(), 1);
    assert!(kernel.take_death_proposal().is_none());
    assert!(matches!(
        kernel.take_character(binding()),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    kernel
        .enqueue(Command::RaiseProgression {
            context: context(3),
            request: bace_gameplay_api::RaiseProgression {
                target: ProgressionTarget::Attribute(AttributeId::Strength),
                amount: 1,
            },
        })
        .unwrap();
    for _ in 0..3 {
        tick(&mut kernel);
    }
    // A reservation returns a retryable pre-sequence rejection: retaining this
    // command at the FIFO head would block the very receipt that releases it.
    assert_eq!(kernel.queued_commands(), 0);
    let blocked = kernel.take_progression_outcome().unwrap();
    assert_eq!(blocked.context, context(3));
    assert_eq!(
        blocked.result,
        Err(bace_gameplay_api::ProgressionActionRejection::DurabilityPending)
    );
    assert!(kernel.take_progression_outcome().is_none());
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        100
    );
    let proposal = kernel.take_death_proposal().unwrap();
    assert_eq!(proposal.owner, Some(EntityId(1)));
    assert_eq!(
        proposal.drops,
        vec![bace_simulation::LootDrop {
            template: 43,
            stack: 2
        }]
    );
    assert_eq!(proposal.experience, vec![(EntityId(1), 100)]);
    assert_eq!(proposal.experience_state[0].1.before_available, 100);
    assert_eq!(proposal.experience_state[0].1.after_available, 200);
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        100
    );
    assert_eq!(
        kernel.confirm_death_committed(proposal.operation, EntityId(1000), &[EntityId(1001)], &[]),
        Err(PveError::InvalidReceipt)
    );
    kernel.retry_death(proposal.operation).unwrap();
    tick(&mut kernel);
    assert_eq!(kernel.take_death_proposal().unwrap(), proposal);
    kernel
        .confirm_death_committed(
            proposal.operation,
            EntityId(1000),
            &[EntityId(1001)],
            &proposal.experience_state,
        )
        .unwrap();
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        200
    );
    assert_eq!(
        kernel.take_pve_event(),
        Some(PveEvent::CorpseCreated {
            operation: proposal.operation,
            corpse: EntityId(1000)
        })
    );
    assert_eq!(
        kernel.world().corpse(EntityId(1000)).unwrap().items,
        [EntityId(1001)]
    );
    assert!(
        kernel
            .confirm_death_committed(
                proposal.operation,
                EntityId(1000),
                &[EntityId(1001)],
                &proposal.experience_state
            )
            .is_err()
    );
    // Retry exactly the rejected sequence after durable adoption, proving that
    // the blocked attempt consumed neither XP nor authenticated action sequence.
    kernel
        .enqueue(Command::RaiseProgression {
            context: context(3),
            request: bace_gameplay_api::RaiseProgression {
                target: ProgressionTarget::Attribute(AttributeId::Strength),
                amount: 1,
            },
        })
        .unwrap();
    for _ in 0..5 {
        tick(&mut kernel);
    }
    let trained = kernel.take_progression_outcome().unwrap();
    assert_eq!(trained.context, context(3));
    assert!(trained.result.is_ok());
    assert!(kernel.world().combatant(EntityId(4)).is_some());
    assert_eq!(kernel.queued_commands(), 0);
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        199
    );
    assert_eq!(
        kernel.take_pve_event(),
        Some(PveEvent::Respawned {
            previous: EntityId(3),
            actor: EntityId(4)
        })
    );
    // Valuable items prevent cleanup until their confirmed ownership transfer.
    assert!(kernel.world().corpse(EntityId(1000)).is_some());
    assert!(kernel.confirm_corpse_item_removed(EntityId(1000), EntityId(1001)));
    tick(&mut kernel);
    assert!(kernel.world().corpse(EntityId(1000)).is_none());
    assert_eq!(
        kernel.take_pve_event(),
        Some(PveEvent::CorpseDecayed {
            corpse: EntityId(1000)
        })
    );
}
#[test]
fn missing_geometry_blocks_npc_admission_without_entity_leak() {
    let mut kernel = kernel(4, 2, 4);
    let mut blueprint = npc();
    blueprint.cell = CellId(999);
    assert_eq!(
        kernel.spawn_npc(EntityId(3), blueprint),
        Err(PveError::MissingGeometry)
    );
    assert!(kernel.world().body(EntityId(3)).is_err());
}
#[test]
fn gdle_home_leash_uses_accepted_ground_and_server_teleport_after_return_timeout() {
    let mut kernel = kernel(16, 2, 16);
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel
        .set_npc_leash(
            EntityId(3),
            bace_ai::MonsterLeash {
                chase_range: 5.0,
                home_range: 3.0,
                arrival_range: 0.25,
                return_timeout_ticks: 3,
            },
        )
        .unwrap();
    tick(&mut kernel); // Establish home from accepted grounded physics state.
    kernel
        .enqueue(Command::ServerTeleport {
            actor: EntityId(3),
            cell: CellId(1),
            position: Vec3::new(8.0, 0.0, 0.5),
        })
        .unwrap();
    tick(&mut kernel);
    let epoch = kernel.world().body(EntityId(3)).unwrap().accepted().epoch();
    for _ in 0..5 {
        tick(&mut kernel);
    }
    let state = kernel.world().body(EntityId(3)).unwrap().accepted();
    assert!((state.position().x - 2.0).abs() < 0.25);
    assert_ne!(state.epoch(), epoch); // Privileged teleport invalidates stale movement.
    assert_eq!(kernel.world().combatant(EntityId(1)).unwrap().health(), 10);
}

fn kill_npc(kernel: &mut Kernel, actor: EntityId, sequence: u32) -> bace_simulation::DeathProposal {
    kernel
        .enqueue(Command::Combat {
            context: context(sequence),
            request: CombatRequest::ChangeMode(2),
        })
        .unwrap();
    kernel
        .enqueue(Command::Combat {
            context: context(sequence + 1),
            request: CombatRequest::TargetedMelee {
                target: actor,
                height: 2,
                power: 0.5,
            },
        })
        .unwrap();
    for _ in 0..16 {
        tick(kernel);
        if let Some(proposal) = kernel.take_death_proposal() {
            return proposal;
        }
    }
    panic!("death proposal did not become ready");
}

#[test]
fn older_valuable_corpse_does_not_block_later_empty_corpse_decay() {
    let mut kernel = kernel(32, 2, 32);
    let mut first = npc();
    first.respawn_ticks = 1000;
    first.corpse_decay_ticks = 0;
    kernel.spawn_npc(EntityId(3), first.clone()).unwrap();
    kernel.supply_loot_random(&[0.8, 0.2]).unwrap();
    let proposal = kill_npc(&mut kernel, EntityId(3), 1);
    kernel
        .confirm_death_committed(
            proposal.operation,
            EntityId(1000),
            &[EntityId(1001)],
            &proposal.experience_state,
        )
        .unwrap();
    tick(&mut kernel);
    assert!(kernel.world().corpse(EntityId(1000)).is_some());

    first.loot.clear();
    kernel.spawn_npc(EntityId(4), first).unwrap();
    let second = kill_npc(&mut kernel, EntityId(4), 3);
    assert!(second.drops.is_empty());
    kernel
        .confirm_death_committed(
            second.operation,
            EntityId(1002),
            &[],
            &second.experience_state,
        )
        .unwrap();
    tick(&mut kernel);
    assert!(kernel.world().corpse(EntityId(1002)).is_none());
    assert_eq!(
        kernel.world().corpse(EntityId(1000)).unwrap().items,
        [EntityId(1001)]
    );
    let mut decayed = Vec::new();
    while let Some(event) = kernel.take_pve_event() {
        if let PveEvent::CorpseDecayed { corpse } = event {
            decayed.push(corpse);
        }
    }
    assert_eq!(decayed, [EntityId(1002)]);
    assert!(kernel.confirm_corpse_item_removed(EntityId(1000), EntityId(1001)));
    tick(&mut kernel);
    assert!(kernel.world().corpse(EntityId(1000)).is_none());
    assert_eq!(
        kernel.take_pve_event(),
        Some(PveEvent::CorpseDecayed {
            corpse: EntityId(1000)
        })
    );
}

#[test]
fn npc_home_keeps_its_cell_even_when_other_cell_has_identical_local_coordinates() {
    let mut kernel = kernel(16, 2, 16);
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel
        .set_npc_leash(
            EntityId(3),
            bace_ai::MonsterLeash {
                chase_range: 5.0,
                home_range: 3.0,
                arrival_range: 0.25,
                return_timeout_ticks: 3,
            },
        )
        .unwrap();
    tick(&mut kernel);
    kernel
        .enqueue(Command::ServerTeleport {
            actor: EntityId(3),
            cell: CellId(2),
            position: Vec3::new(2.0, 0.0, 0.5),
        })
        .unwrap();
    tick(&mut kernel);
    let (_, cell, state) = kernel
        .world()
        .states()
        .find(|(id, _, _)| *id == EntityId(3))
        .unwrap();
    assert_eq!(cell, CellId(2));
    let epoch = state.epoch();
    for _ in 0..5 {
        tick(&mut kernel);
    }
    let (_, cell, state) = kernel
        .world()
        .states()
        .find(|(id, _, _)| *id == EntityId(3))
        .unwrap();
    assert_eq!(cell, CellId(1));
    assert!((state.position().x - 2.0).abs() < 0.25);
    assert_ne!(state.epoch(), epoch);
}

fn reserved_id_door() -> bace_simulation::PreparedDoor {
    bace_simulation::PreparedDoor {
        collider: bace_world::DoorCollider {
            cell: CellId(1),
            position: Vec3::new(5.1, 0.0, 0.5),
            bounds: Aabb::new(Vec3::new(5.0, -1.0, 0.0), Vec3::new(5.2, 1.0, 2.0)).unwrap(),
            solid: true,
        },
        initially_open: false,
        initially_locked: false,
        reset_interval_ticks: None,
        use_radius: 5.0,
        open: bace_simulation::PreparedDoorAnimation {
            duration_ticks: 2,
            hooks: vec![bace_simulation::PreparedDoorHook {
                offset_ticks: 1,
                ethereal: true,
            }],
        },
        close: bace_simulation::PreparedDoorAnimation {
            duration_ticks: 2,
            hooks: vec![bace_simulation::PreparedDoorHook {
                offset_ticks: 1,
                ethereal: false,
            }],
        },
    }
}

#[test]
fn supplied_respawn_and_pending_generator_ids_cannot_be_stolen_by_manual_admission() {
    let mut kernel = kernel(16, 2, 16);
    kernel.supply_spawn_id(EntityId(7)).unwrap();
    assert_eq!(
        kernel.spawn_npc(EntityId(7), npc()),
        Err(PveError::Duplicate)
    );
    assert_eq!(
        kernel.register_door(EntityId(7), reserved_id_door()),
        Err(bace_gameplay_api::DoorRejection::InvalidState)
    );
    assert!(kernel.world().body(EntityId(7)).is_err());
    assert!(kernel.world().door(EntityId(7)).is_none());
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel.supply_loot_random(&[0.8]).unwrap();
    let proposal = kill_npc(&mut kernel, EntityId(3), 1);
    kernel
        .confirm_death_committed(
            proposal.operation,
            EntityId(1000),
            &[EntityId(1001)],
            &proposal.experience_state,
        )
        .unwrap();
    assert!(kernel.world().body(EntityId(3)).is_err());
    assert_eq!(
        kernel.spawn_npc(EntityId(3), npc()),
        Err(PveError::Duplicate)
    );
    assert_eq!(
        kernel.supply_spawn_id(EntityId(3)),
        Err(PveError::Duplicate)
    );
    assert_eq!(
        kernel.register_door(EntityId(3), reserved_id_door()),
        Err(bace_gameplay_api::DoorRejection::InvalidState)
    );
    for _ in 0..5 {
        tick(&mut kernel);
    }
    assert!(kernel.world().combatant(EntityId(7)).is_some());
    let mut respawned = false;
    while let Some(event) = kernel.take_pve_event() {
        respawned |= event
            == PveEvent::Respawned {
                previous: EntityId(3),
                actor: EntityId(7),
            };
    }
    assert!(respawned);
}

#[test]
fn impossible_timer_profiles_reject_before_entity_or_generator_admission() {
    let mut kernel = kernel(16, 2, 16);
    for value in [u64::from(u32::MAX) + 1, u64::MAX] {
        for field in 0..4 {
            let mut blueprint = npc();
            match field {
                0 => blueprint.think_interval = value,
                1 => blueprint.death_animation_ticks = value,
                2 => blueprint.respawn_ticks = value,
                _ => blueprint.corpse_decay_ticks = value,
            }
            assert_eq!(
                kernel.spawn_npc(EntityId(3), blueprint),
                Err(PveError::InvalidProfile)
            );
            assert!(kernel.world().body(EntityId(3)).is_err());
            assert!(!kernel.has_pve_state());
        }
    }
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    assert_eq!(
        kernel.set_npc_leash(
            EntityId(3),
            bace_ai::MonsterLeash {
                return_timeout_ticks: u64::MAX,
                ..Default::default()
            }
        ),
        Err(PveError::InvalidProfile)
    );
}

#[test]
fn shared_death_reserves_full_xp_and_refuses_legacy_receipt() {
    let mut kernel = kernel(32, 4, 32);
    kernel
        .register_npc_character_services(
            EntityId(1),
            bace_character::CharacterServiceState {
                level: 1,
                total_experience: 0,
                total_skill_credits: None,
                titles: vec![],
                enlightenment: 0,
                sanctuary: None,
            },
            bace_quests::ContractRegistry::restore(vec![]).unwrap(),
        )
        .unwrap();
    kernel
        .configure_social_experience(Arc::new(
            bace_character::CharacterLevelTable::prepare(vec![0, 0, 100, 300], vec![0, 0, 0, 0])
                .unwrap(),
        ))
        .unwrap();
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel.supply_loot_random(&[0.8, 0.2]).unwrap();
    let proposal = kill_npc(&mut kernel, EntityId(3), 1);
    let social = proposal.social.clone().expect("shared reward attachment");
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        100
    );
    assert!(kernel.take_allegiance_proposal().is_none());
    assert!(
        kernel
            .confirm_death_committed(
                proposal.operation,
                EntityId(20),
                &[EntityId(21)],
                &proposal.experience_state
            )
            .is_err()
    );
    assert!(kernel.peek_experience_event().is_none());
    let items: Vec<_> = (0..proposal.drops.len())
        .map(|i| EntityId(21 + i as u32))
        .collect();
    kernel
        .confirm_shared_death_committed(
            proposal.operation,
            EntityId(20),
            &items,
            &proposal.experience_state,
            &social,
        )
        .unwrap();
    let event = kernel
        .take_experience_event()
        .expect("durable XP publication");
    assert_eq!(event.actor, EntityId(1));
    assert_eq!(event.before.available, 100);
    assert_eq!(
        event.after.available,
        100 + social.player_changes[0].1.credited
    );
    assert!(event.update_properties);
    assert!(kernel.peek_experience_event().is_none());
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        100 + social.player_changes[0].1.credited
    );
    assert!(
        kernel
            .confirm_shared_death_committed(
                proposal.operation,
                EntityId(20),
                &items,
                &proposal.experience_state,
                &social
            )
            .is_err()
    );
}
