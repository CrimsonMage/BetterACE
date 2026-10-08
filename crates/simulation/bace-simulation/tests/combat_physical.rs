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

#[path = "physical/resources.rs"]
mod resources;
#[path = "physical/motion.rs"]
mod trusted_motion;

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
    for id in 1..=3 {
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
    for id in 1..=3 {
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 20,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: id == 1,
                })
                .unwrap()
                .with_resources(
                    Some(bace_entity::VitalPool {
                        current: 100,
                        maximum: 100,
                    }),
                    None,
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

fn combat(sequence: u32, request: CombatRequest) -> Command {
    Command::Combat {
        context: context(sequence),
        request,
    }
}
fn attack(sequence: u32) -> Command {
    combat(
        sequence,
        CombatRequest::TargetedMelee {
            target: EntityId(2),
            height: 2,
            power: 0.5,
        },
    )
}
use bace_gameplay_api::weapon_combat::*;
fn prepared(player: bool) -> PhysicalCombatProfile {
    let weapon = PhysicalWeapon {
        entity: if player { 100 } else { 200 },
        revision: 1,
        style: 2,
        attack_type: 6,
        damage_type: 3,
        damage: 20.0,
        variance: 0.0,
        skill: 44,
        attack_time: 0,
        encumbrance: 0,
        offense: 1.0,
        imbues: 0,
        biting: 0.0,
        crushing: 0.0,
        slayer_type: 0,
        slayer_bonus: 0.0,
        cleave_targets: 1,
        ignore_magic_armor: false,
        ignore_magic_resistance: false,
        armor_cleaving: false,
        resistance_cleaving: None,
        proc_spell: None,
        proc_chance: 0.0,
    };
    let armor = PhysicalBodyDefense {
        part: 0,
        hit_weights: [1.0; 12],
        armor: [quality(0.0); 8],
    };
    PhysicalCombatProfile {
        equipment: vec![PhysicalEquipmentStamp {
            entity: if player { 100 } else { 200 },
            revision: 1,
            location: 0x100000,
        }],
        revision: 1,
        content_hash: [7; 32],
        player,
        creature_type: 1,
        pk: PkStatus::Npk,
        attackable: true,
        immune: false,
        lifestone_protected: false,
        style: 0x80000040,
        main: Some(weapon),
        offhand: None,
        launcher: None,
        ammunition: None,
        gloves: None,
        boots: None,
        body_attacks: vec![],
        maneuvers: vec![PhysicalManeuver {
            style: 0x80000040,
            attack_type: 4,
            height: 2,
            minimum_skill: 0,
            motion: 0x10000063,
            duration: 0.2,
            hooks: vec![
                PhysicalAttackHook {
                    seconds: 0.05,
                    part: 0,
                },
                PhysicalAttackHook {
                    seconds: 0.1,
                    part: 0,
                },
            ],
        }],
        skills: vec![
            (
                6,
                PhysicalSkill {
                    advancement: 2,
                    current: 100,
                },
            ),
            (
                44,
                PhysicalSkill {
                    advancement: 3,
                    current: 300,
                },
            ),
        ],
        strength: 100,
        coordination: 100,
        quickness: 100,
        base_strength: 0,
        base_endurance: 0,
        melee_defense_modifier: 1.0,
        missile_defense_modifier: 1.0,
        armor: vec![armor],
        resistances: [PhysicalResistance {
            quality: quality(1.0),
            augmentation: 0,
        }; 8],
        shield_encumbrance: 0,
        shield_placement: true,
        armor_layers: vec![],
        ignore_shield: 0.0,
        shield: None,
        shield_skill: PhysicalSkill {
            advancement: 0,
            current: 0,
        },
        ratings: PhysicalRatings::default(),
        critical_defense: false,
        range: 3.0,
        height: 1.0,
        missile: None,
    }
}
fn native() -> Kernel {
    let mut k = kernel(16, 4, 16);
    k.configure_combat_random_shared(
        Arc::new(bace_random::RandomRoot::new([9; 32], 1).unwrap()),
        1,
    )
    .unwrap();
    for actor in 1..=3 {
        k.register_inventory_container(bace_inventory::InventoryContainer {
            id: EntityId(actor),
            revision: 1,
            root_owner: Some(EntityId(actor)),
            slots: 16,
            pack_slots: 4,
            burden_limit: 10000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    }
    gear(&mut k, 1, 100, 1, 0x100000, 1);
    gear(&mut k, 2, 200, 1, 0x100000, 1);
    k.register_physical_combat(EntityId(1), Arc::new(prepared(true)))
        .unwrap();
    k.register_physical_combat(EntityId(2), Arc::new(prepared(false)))
        .unwrap();
    k
}
#[test]
fn prepared_weapon_hooks_change_world_health_and_report_damage_type() {
    let mut k = native();
    k.enqueue(combat(1, CombatRequest::ChangeMode(2))).unwrap();
    k.step().unwrap();
    k.take_combat_outcome();
    k.enqueue(attack(2)).unwrap();
    for _ in 0..5 {
        k.step().unwrap();
    }
    assert!(matches!(
        k.take_physical_combat_event(),
        Some(bace_simulation::PhysicalCombatEvent::Motion { .. })
    ));
    let bace_simulation::PhysicalCombatEvent::Impact { impact: first, .. } =
        k.take_physical_combat_event().expect("first actual hook")
    else {
        panic!("impact")
    };
    let bace_simulation::PhysicalCombatEvent::Impact { impact: second, .. } =
        k.take_physical_combat_event().expect("second actual hook")
    else {
        panic!("impact")
    };
    assert_eq!(first.damage_type, 1);
    assert_eq!(second.damage_type, 1);
    assert!(first.damage > 0);
    assert_eq!(
        k.world().combatant(EntityId(2)).unwrap().health(),
        100 - first.damage - second.damage
    );
    // Pinned GDLE AttackManager::OnAttackDone notifies once when no repeat is queued.
    assert!(matches!(
        k.take_physical_combat_event(),
        Some(bace_simulation::PhysicalCombatEvent::AttackDone {
            actor: EntityId(1),
            ..
        })
    ));
    assert_eq!(
        k.take_physical_combat_event(),
        None,
        "only one terminal completion"
    );
}

#[test]
fn prepared_npk_players_cannot_damage_each_other() {
    let mut k = native();
    k.register_physical_combat(EntityId(2), Arc::new(prepared(true)))
        .unwrap();
    k.enqueue(combat(1, CombatRequest::ChangeMode(2))).unwrap();
    k.step().unwrap();
    k.take_combat_outcome();
    k.enqueue(attack(2)).unwrap();
    k.step().unwrap();
    assert!(k.take_combat_outcome().unwrap().result.is_err());
    assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 100);
}
#[test]
fn missile_ammunition_receipt_gates_world_projectile_and_actual_impact() {
    let mut k = native();
    let mut p = prepared(true);
    let mut launcher = p.main.clone().unwrap();
    launcher.entity = 101;
    let mut ammunition = launcher.clone();
    ammunition.entity = 202;
    ammunition.revision = 5;
    ammunition.damage = 5.0;
    ammunition.damage_type = 2;
    p.equipment.push(PhysicalEquipmentStamp {
        entity: 101,
        revision: 1,
        location: 0x400000,
    });
    p.equipment.push(PhysicalEquipmentStamp {
        entity: 202,
        revision: 5,
        location: 0x800000,
    });
    gear(&mut k, 1, 101, 1, 0x400000, 1);
    gear(&mut k, 1, 202, 5, 0x800000, 2);
    p.launcher = Some(launcher);
    p.ammunition = Some(ammunition);
    p.missile = Some(PhysicalMissileSpec {
        ammunition_count: 2,
        speed: 20.0,
        radius: 0.05,
        gravity: false,
        tracking: false,
        attack_motion: 0x10000062,
        launch_seconds: 0.01,
        duration_seconds: 2.0,
        damage_modifier: 1.0,
    });
    k.register_physical_combat(EntityId(1), Arc::new(p))
        .unwrap();
    k.supply_physical_projectile_id(EntityId(1000)).unwrap();
    // All three commands precede the launch tick: leaving missile mode cancels
    // only the unsubmitted launch while retaining its animation admission fence.
    k.enqueue(combat(1, CombatRequest::ChangeMode(4))).unwrap();
    k.enqueue(combat(
        2,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 3,
            accuracy: 0.5,
        },
    ))
    .unwrap();
    k.enqueue(combat(3, CombatRequest::ChangeMode(1))).unwrap();
    k.step().unwrap();
    assert!(k.take_physical_launch().is_none());
    assert!(k.world().projectile(EntityId(1000)).is_none());
    assert!(k.physical_recovery_pending(EntityId(1)));
    assert_eq!(
        k.physical_combat_profile(EntityId(1))
            .unwrap()
            .missile
            .unwrap()
            .ammunition_count,
        2
    );
    while k.take_combat_outcome().is_some() {}
    for _ in 0..100 {
        k.step().unwrap();
    }
    assert!(!k.physical_recovery_pending(EntityId(1)));
    k.supply_physical_projectile_id(EntityId(1000)).unwrap();
    k.enqueue(combat(4, CombatRequest::ChangeMode(4))).unwrap();
    k.enqueue(combat(
        5,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 3,
            accuracy: 0.5,
        },
    ))
    .unwrap();
    k.step().unwrap();
    let proposal = k.take_physical_launch().expect("durable ammo intent");
    assert_eq!(proposal.ammunition, 202);
    assert_eq!(proposal.expected_count, 2);
    k.retry_physical_launch(proposal.operation).unwrap();
    k.retry_physical_launch(proposal.operation).unwrap();
    assert_eq!(k.take_physical_launch().unwrap(), proposal);
    assert!(k.take_physical_launch().is_none());
    while k.take_combat_outcome().is_some() {}
    for (i, request) in [
        CombatRequest::CancelAttack,
        CombatRequest::ChangeMode(1),
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 3,
            accuracy: 0.5,
        },
    ]
    .into_iter()
    .enumerate()
    {
        k.enqueue(combat(i as u32 + 6, request)).unwrap();
    }
    k.step().unwrap();
    while let Some(outcome) = k.take_combat_outcome() {
        assert_eq!(
            outcome.result,
            Err(bace_gameplay_api::CombatRejection::Busy)
        );
    }
    assert_eq!(k.world().combatant(EntityId(1)).unwrap().mode(), 4);

    assert!(k.world().projectile(EntityId(1000)).is_none());
    assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 100);
    let mut receipt = PhysicalLaunchReceipt {
        operation: proposal.operation,
        ammunition: 202,
        before_revision: 5,
        after_revision: 7,
        remaining: 1,
    };
    assert!(k.confirm_physical_launch(receipt).is_err());
    receipt.after_revision = 6;
    // Simulates the adapter adopting the matching combined durable ammo receipt.
    // Database atomicity is covered by runtime's valuable-operation tests.
    let ammo_operation = k.propose_item_take(EntityId(1), EntityId(202), 1).unwrap();
    let ammo_ticket = k.take_inventory_proposal().unwrap();
    assert_eq!(ammo_ticket.operation, ammo_operation);
    k.confirm_inventory_committed(&bace_simulation::InventoryReceipt {
        operation: ammo_operation,
        revisions: ammo_ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    k.confirm_physical_launch(receipt).unwrap();
    assert!(k.confirm_physical_launch(receipt).is_err());
    assert_eq!(
        k.physical_combat_profile(EntityId(1))
            .unwrap()
            .equipment
            .iter()
            .find(|s| s.entity == 202)
            .unwrap()
            .revision,
        6
    );
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().revision, 6);
    for _ in 0..8 {
        k.step().unwrap();
    }
    assert!(k.world().projectile(EntityId(1000)).is_none());
    assert!(k.world().combatant(EntityId(2)).unwrap().health() < 100);
    assert!(k.physical_recovery_pending(EntityId(1)));
    k.supply_physical_projectile_id(EntityId(1001)).unwrap();
    for sequence in 9..=18 {
        k.enqueue(combat(
            sequence,
            CombatRequest::TargetedMissile {
                target: EntityId(2),
                height: 3,
                accuracy: 0.5,
            },
        ))
        .unwrap();
    }
    let after_impact = k.world().combatant(EntityId(2)).unwrap().health();
    k.step().unwrap();
    while let Some(outcome) = k.take_combat_outcome() {
        assert_eq!(
            outcome.result,
            Err(bace_gameplay_api::CombatRejection::Busy)
        );
    }
    assert!(k.take_physical_launch().is_none());
    assert!(k.world().projectile(EntityId(1001)).is_none());
    assert_eq!(
        k.world().combatant(EntityId(2)).unwrap().health(),
        after_impact
    );

    assert_eq!(
        k.physical_combat_profile(EntityId(1))
            .unwrap()
            .missile
            .unwrap()
            .ammunition_count,
        1
    );
    for _ in 0..100 {
        k.step().unwrap();
    }
    k.enqueue(combat(
        19,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 3,
            accuracy: 0.5,
        },
    ))
    .unwrap();
    k.step().unwrap();
    assert!(k.take_combat_outcome().unwrap().result.is_ok());
    let last = k.take_physical_launch().unwrap();
    assert_eq!(last.expected_count, 1);
    assert_eq!(last.ammunition_revision, 6);
    let op = k.propose_item_take(EntityId(1), EntityId(202), 1).unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    k.confirm_inventory_committed(&bace_simulation::InventoryReceipt {
        operation: op,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    k.confirm_physical_launch(PhysicalLaunchReceipt {
        operation: last.operation,
        ammunition: 202,
        before_revision: 6,
        after_revision: 7,
        remaining: 0,
    })
    .unwrap();
    assert!(
        !k.physical_combat_profile(EntityId(1))
            .unwrap()
            .equipment
            .iter()
            .any(|s| s.entity == 202)
    );
    assert_eq!(
        k.physical_combat_profile(EntityId(1))
            .unwrap()
            .missile
            .unwrap()
            .ammunition_count,
        0
    );
}
#[test]
fn cleave_and_multistrike_each_damage_authoritative_eligible_targets_once() {
    let mut k = native();
    let mut p = prepared(true);
    p.main.as_mut().unwrap().cleave_targets = 2;
    k.register_physical_combat(EntityId(1), Arc::new(p))
        .unwrap();
    let mut third = prepared(false);
    third.main.as_mut().unwrap().entity = 300;
    third.equipment[0].entity = 300;
    gear(&mut k, 3, 300, 1, 0x100000, 1);
    k.register_physical_combat(EntityId(3), Arc::new(third))
        .unwrap();
    k.enqueue(combat(1, CombatRequest::ChangeMode(2))).unwrap();
    k.enqueue(attack(2)).unwrap();
    for _ in 0..6 {
        k.step().unwrap();
    }
    let mut totals = std::collections::BTreeMap::new();
    let mut strikes = 0;
    while let Some(event) = k.take_physical_combat_event() {
        if let bace_simulation::PhysicalCombatEvent::Impact { target, impact, .. } = event {
            *totals.entry(target).or_insert(0) += impact.damage;
            strikes += 1;
        }
    }
    assert_eq!(strikes, 4);
    for id in [EntityId(2), EntityId(3)] {
        assert_eq!(k.world().combatant(id).unwrap().health(), 100 - totals[&id]);
        assert!(totals[&id] > 0);
    }
}

fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    }
}

fn gear(k: &mut Kernel, owner: u32, id: u32, revision: u64, location: u32, stack: u32) {
    k.register_inventory_item(bace_inventory::InventoryItem {
        structure: None,
        id: EntityId(id),
        revision,
        template: 100,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(owner),
            slot: 0,
            equipped: location,
        },
        stack,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: location,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
}

#[test]
fn native_npc_launches_infinite_ammunition_without_external_receipt() {
    let mut k = native();
    k.spawn_npc(
        EntityId(4),
        bace_simulation::NpcBlueprint {
            cell: CellId(1),
            position: Vec3::new(-5.0, 0.0, 0.5),
            radius: 0.5,
            capabilities: Capabilities {
                speed: 1.0,
                jump_impulse: 0.0,
            },
            combat: CombatantProfile {
                maximum_health: 100,
                melee_damage: 2,
                melee_range: 1.0,
                attack_duration: 0.1,
                strike_offsets: vec![0.05],
                player: false,
            },
            visual_range: 18.0,
            think_interval: 1,
            corpse_template: 999,
            xp_override: None,
            loot: vec![],
            death_animation_ticks: 1,
            respawn_ticks: 0,
            corpse_decay_ticks: 30,
        },
    )
    .unwrap();
    k.register_inventory_container(bace_inventory::InventoryContainer {
        id: EntityId(4),
        revision: 1,
        root_owner: None,
        slots: 16,
        pack_slots: 4,
        burden_limit: 10000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    let mut p = prepared(false);
    p.equipment.clear();
    let mut launcher = p.main.take().unwrap();
    launcher.entity = 401;
    let mut ammunition = launcher.clone();
    ammunition.entity = 402;
    ammunition.revision = 5;
    ammunition.damage = 5.0;
    ammunition.damage_type = 2;
    p.equipment.extend([
        PhysicalEquipmentStamp {
            entity: 401,
            revision: 1,
            location: 0x400000,
        },
        PhysicalEquipmentStamp {
            entity: 402,
            revision: 5,
            location: 0x800000,
        },
    ]);
    gear(&mut k, 4, 401, 1, 0x400000, 1);
    gear(&mut k, 4, 402, 5, 0x800000, 2);
    p.launcher = Some(launcher);
    p.ammunition = Some(ammunition);
    p.missile = Some(PhysicalMissileSpec {
        ammunition_count: 2,
        speed: 20.0,
        radius: 0.05,
        gravity: false,
        tracking: false,
        attack_motion: 0x10000062,
        launch_seconds: 0.01,
        duration_seconds: 0.1,
        damage_modifier: 1.0,
    });
    k.register_physical_combat(EntityId(4), Arc::new(p))
        .unwrap();
    k.supply_physical_projectile_id(EntityId(1000)).unwrap();
    let mut launched = false;
    for _ in 0..30 {
        k.step().unwrap();
        assert!(
            k.take_physical_launch().is_none(),
            "NPC needs no durable ammunition receipt"
        );
        launched |= k.world().projectile(EntityId(1000)).is_some();
        while k.take_combat_event().is_some() {}
    }
    assert!(launched, "AI chose its ranged profile beyond melee reach");
    assert!(
        k.world().combatant(EntityId(1)).unwrap().health() < 100,
        "accepted projectile hit authoritative player health"
    );
    assert_eq!(k.inventory_item(EntityId(402)).unwrap().stack, 2);
    assert_eq!(k.inventory_item(EntityId(402)).unwrap().revision, 5);
    assert_eq!(
        k.physical_combat_profile(EntityId(4))
            .unwrap()
            .missile
            .unwrap()
            .ammunition_count,
        2
    );
}

#[test]
fn prepared_attack_input_bounds_reject_without_damage_stamina_motion_or_launch() {
    for missile in [false, true] {
        let mut k = native();
        k.enqueue(combat(
            1,
            CombatRequest::ChangeMode(if missile { 4 } else { 2 }),
        ))
        .unwrap();
        k.step().unwrap();
        k.take_combat_outcome();
        let before = k
            .world()
            .combatant(EntityId(1))
            .unwrap()
            .vital(bace_entity::EntityVital::Stamina);
        for (index, power) in [
            f32::from_bits(0x3f800001),
            -f32::from_bits(1),
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ]
        .into_iter()
        .enumerate()
        {
            k.enqueue(combat(
                index as u32 + 2,
                if missile {
                    CombatRequest::TargetedMissile {
                        target: EntityId(2),
                        height: 2,
                        accuracy: power,
                    }
                } else {
                    CombatRequest::TargetedMelee {
                        target: EntityId(2),
                        height: 2,
                        power,
                    }
                },
            ))
            .unwrap();
            k.step().unwrap();
            assert_eq!(
                k.take_combat_outcome().unwrap().result,
                Err(bace_gameplay_api::CombatRejection::InvalidRequest)
            );
            assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 100);
            assert_eq!(
                k.world()
                    .combatant(EntityId(1))
                    .unwrap()
                    .vital(bace_entity::EntityVital::Stamina),
                before
            );
            assert!(k.take_physical_combat_event().is_none());
            assert!(k.take_physical_launch().is_none());
            assert!(!k.physical_recovery_pending(EntityId(1)));
        }
    }
}
#[test]
fn fresh_sequence_flood_and_cancel_mode_cycles_cannot_replay_early_melee_hooks() {
    let mut k = native();
    let mut p = prepared(true);
    p.maneuvers[0].duration = 2.0;
    p.maneuvers[0].hooks = vec![PhysicalAttackHook {
        seconds: 0.01,
        part: 0,
    }];
    k.register_physical_combat(EntityId(1), Arc::new(p))
        .unwrap();
    k.enqueue(combat(1, CombatRequest::ChangeMode(2))).unwrap();
    for sequence in 2..=16 {
        k.enqueue(attack(sequence)).unwrap();
    }
    k.step().unwrap();
    let mut starts = 0;
    while let Some(outcome) = k.take_combat_outcome() {
        if matches!(
            outcome.result,
            Ok(bace_gameplay_api::CombatChange::AttackStarted { .. })
        ) {
            starts += 1;
        }
    }
    assert_eq!(starts, 1);
    let health = k.world().combatant(EntityId(2)).unwrap().health();
    assert!(health < 100);
    let mut sequence = 17;
    for _ in 0..10 {
        for request in [
            CombatRequest::CancelAttack,
            CombatRequest::ChangeMode(1),
            CombatRequest::ChangeMode(2),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5,
            },
        ] {
            k.enqueue(combat(sequence, request)).unwrap();
            sequence += 1;
        }
        k.step().unwrap();
        let mut last = None;
        while let Some(outcome) = k.take_combat_outcome() {
            last = Some(outcome);
        }
        assert_eq!(
            last.unwrap().result,
            Err(bace_gameplay_api::CombatRejection::Busy)
        );
        assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), health);
        assert!(k.physical_recovery_pending(EntityId(1)));
        assert!(k.capture_physical_recovery(EntityId(1)) > 0.0);
    }
    for _ in 0..100 {
        k.step().unwrap();
        while k.take_combat_event().is_some() {}
    }
    assert!(!k.physical_recovery_pending(EntityId(1)));
    k.enqueue(attack(sequence)).unwrap();
    k.step().unwrap();
    assert!(k.take_combat_outcome().unwrap().result.is_ok());
    assert!(k.world().combatant(EntityId(2)).unwrap().health() < health);
}
