//! Reservations protect delayed physical work without spending hooks or hits.
use super::*;
use bace_entity::{Actor, Combatant, CombatantProfile, EntityVital, VitalPool};
use bace_gameplay_api::weapon_combat::*;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::CellId;
use bace_world::{VitalReservationDomain, VitalReservationToken};
use std::sync::Arc;
fn fixture() -> (Combat, World) {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=3 {
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body: Body::spawn(
                    world.scene(cell).unwrap(),
                    Vec3::new(id as f32, 0., 0.5),
                    0.5,
                    Capabilities {
                        speed: 0.,
                        jump_impulse: 0.,
                    },
                )
                .unwrap(),
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 20,
                    melee_range: 2.,
                    attack_duration: 0.5,
                    strike_offsets: vec![0.05],
                    player: id == 1,
                })
                .unwrap()
                .with_resources(
                    Some(VitalPool {
                        current: 100,
                        maximum: 100,
                    }),
                    Some(VitalPool {
                        current: 100,
                        maximum: 100,
                    }),
                )
                .unwrap(),
            )
            .unwrap();
    }
    let mut combat = Combat::new(16);
    combat
        .configure_random(
            Arc::new(bace_random::RandomRoot::new([9; 32], 1).unwrap()),
            1,
        )
        .unwrap();
    (combat, world)
}
fn token() -> VitalReservationToken {
    VitalReservationToken {
        domain: VitalReservationDomain::NpcRetirement,
        operation: 44,
    }
}
#[test]
fn basic_hook_waits_for_reserved_health_and_resumes_once() {
    let (mut combat, mut world) = fixture();
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(2), 0.0)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5,
            },
            0.0,
        )
        .unwrap();
    world
        .reserve_vitals(&[(EntityId(2), EntityVital::Health)], token())
        .unwrap();
    for now in [0.1, 0.2, 0.4] {
        combat.step_internal(&mut world, now, None);
        assert_eq!(
            world
                .vital(EntityId(2), EntityVital::Health)
                .unwrap()
                .current,
            100
        );
    }
    world.release_vitals(token());
    combat.step_internal(&mut world, 0.45, None);
    assert!(world.combatant(EntityId(2)).unwrap().health() < 100);
    let health = world.combatant(EntityId(2)).unwrap().health();
    combat.step_internal(&mut world, 0.6, None);
    assert_eq!(world.combatant(EntityId(2)).unwrap().health(), health);
}
#[test]
fn physical_cleave_is_retained_as_one_hook_when_one_target_is_reserved() {
    let (mut combat, mut world) = fixture();
    for id in 1..=3 {
        let mut p = prepared(id == 1);
        p.equipment[0].entity = id * 100;
        p.main.as_mut().unwrap().entity = id * 100;
        p.maneuvers[0].hooks.truncate(1);
        if id == 1 {
            p.main.as_mut().unwrap().cleave_targets = 3;
        }
        combat
            .register_physical(EntityId(id), Arc::new(p), &world)
            .unwrap();
    }
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(2), 0.0)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5,
            },
            0.0,
        )
        .unwrap();
    world
        .reserve_vitals(&[(EntityId(3), EntityVital::Health)], token())
        .unwrap();
    for now in [0.1, 0.2, 1.0] {
        combat.step_internal(&mut world, now, None);
        assert_eq!(world.combatant(EntityId(2)).unwrap().health(), 100);
        assert_eq!(world.combatant(EntityId(3)).unwrap().health(), 100);
        assert_eq!(
            world
                .vital(EntityId(1), EntityVital::Stamina)
                .unwrap()
                .current,
            100
        );
    }
    world.release_vitals(token());
    combat.step_internal(&mut world, 1.1, None);
    assert!(world.combatant(EntityId(2)).unwrap().health() < 100);
    assert!(world.combatant(EntityId(3)).unwrap().health() < 100);
    let before = (
        world.combatant(EntityId(2)).unwrap().health(),
        world.combatant(EntityId(3)).unwrap().health(),
    );
    combat.step_internal(&mut world, 1.2, None);
    assert_eq!(
        before,
        (
            world.combatant(EntityId(2)).unwrap().health(),
            world.combatant(EntityId(3)).unwrap().health()
        )
    );
}
#[test]
fn missile_contact_survives_reservation_past_lifetime_and_retirement_pressure() {
    for retired in [false, true] {
        let (mut combat, mut world) = fixture();
        let mut p = prepared(true);
        let mut launcher = p.main.clone().unwrap();
        launcher.entity = 101;
        let mut ammo = launcher.clone();
        ammo.entity = 202;
        ammo.revision = 5;
        ammo.damage_type = 2;
        ammo.damage = 5.;
        p.launcher = Some(launcher);
        p.ammunition = Some(ammo);
        p.missile = Some(PhysicalMissileSpec {
            ammunition_count: 1,
            speed: 20.,
            radius: 0.1,
            gravity: false,
            tracking: false,
            attack_motion: 0x4000001e,
            launch_seconds: 0.0,
            duration_seconds: 0.5,
            damage_modifier: 1.0,
        });
        combat
            .register_physical(EntityId(1), Arc::new(p), &world)
            .unwrap();
        combat
            .register_physical(EntityId(2), Arc::new(prepared(false)), &world)
            .unwrap();
        combat.supply_missile_id(EntityId(1000), &world).unwrap();
        combat
            .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(4), 0.0)
            .unwrap();
        combat
            .apply(
                &mut world,
                EntityId(1),
                CombatRequest::TargetedMissile {
                    target: EntityId(2),
                    height: 2,
                    accuracy: 0.5,
                },
                0.0,
            )
            .unwrap();
        combat.step_internal(&mut world, 0.1, None);
        let launch = combat.take_launch().unwrap();
        assert!(combat.references_region(0, &world));
        assert!(!combat.references_region(1, &world));
        combat
            .confirm_launch(
                PhysicalLaunchReceipt {
                    operation: launch.operation,
                    ammunition: 202,
                    before_revision: 5,
                    after_revision: 6,
                    remaining: 0,
                },
                &mut world,
            )
            .unwrap();
        world
            .reserve_vitals(&[(EntityId(2), EntityVital::Health)], token())
            .unwrap();
        for now in [0.2, 0.3, 10.0] {
            combat.step_internal(&mut world, now, None);
            assert_eq!(world.combatant(EntityId(2)).unwrap().health(), 100);
        }
        assert!(combat.missiles[&launch.operation].pending_contact.is_some());
        assert!(!combat.retire_region_projectiles(&mut world, 0));
        assert!(world.projectile(EntityId(1000)).is_some());
        world.release_vitals(token());
        if retired {
            world.remove(EntityId(2)).unwrap();
        }
        combat.step_internal(&mut world, 10.1, None);
        if !retired {
            assert!(world.combatant(EntityId(2)).unwrap().health() < 100);
        } else {
            assert!(
                !combat
                    .events
                    .iter()
                    .any(|e| matches!(e, CombatEvent::Damage { .. }))
            );
        }
        combat.step_internal(&mut world, 10.2, None);
        assert!(combat.missiles.is_empty());
        assert!(!combat.references_region(0, &world));
        assert!(world.projectile(EntityId(1000)).is_none());
    }
}

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
fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    }
}
fn make_player(world: &mut World, id: EntityId) {
    let actor = world.remove(id).unwrap();
    world.insert(actor).unwrap();
    world
        .register_combatant(
            id,
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 20,
                melee_range: 2.,
                attack_duration: 0.5,
                strike_offsets: vec![0.05],
                player: true,
            })
            .unwrap()
            .with_resources(
                Some(VitalPool {
                    current: 100,
                    maximum: 100,
                }),
                Some(VitalPool {
                    current: 100,
                    maximum: 100,
                }),
            )
            .unwrap(),
        )
        .unwrap();
}
#[test]
fn retained_melee_attempt_marks_both_pk_players_even_when_evaded_only_after_admission() {
    let (mut combat, mut world) = fixture();
    make_player(&mut world, EntityId(2));
    for id in [1, 2] {
        let mut profile = prepared(true);
        profile.pk = PkStatus::Pk;
        profile.equipment[0].entity = id * 100;
        profile.main.as_mut().unwrap().entity = id * 100;
        if id == 2 {
            profile
                .skills
                .iter_mut()
                .find(|(id, _)| *id == 6)
                .unwrap()
                .1
                .current = 100000;
        }
        combat
            .register_physical(EntityId(id), Arc::new(profile), &world)
            .unwrap();
    }
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(2), 0.)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5,
            },
            0.,
        )
        .unwrap();
    world
        .reserve_vitals(&[(EntityId(2), EntityVital::Health)], token())
        .unwrap();
    combat.step_internal(&mut world, 1., None);
    assert!(!world.combatant(EntityId(1)).unwrap().pk_activity_active(1.));
    world.release_vitals(token());
    combat.step_internal(&mut world, 5.1, None);
    assert!(combat.physical_events.iter().any(
        |event| matches!(event,physical::PhysicalCombatEvent::Impact{impact,..} if impact.evaded)
    ));
    for id in [1, 2] {
        let player = world.combatant(EntityId(id)).unwrap();
        assert!(player.pk_activity_active(24.999));
        assert!(!player.pk_activity_active(25.));
    }
}
#[test]
fn evaded_missile_does_not_start_pk_timer() {
    let (mut combat, mut world) = fixture();
    make_player(&mut world, EntityId(2));
    let mut source = prepared(true);
    source.pk = PkStatus::Pk;
    let mut launcher = source.main.clone().unwrap();
    launcher.entity = 101;
    let mut ammo = launcher.clone();
    ammo.entity = 202;
    ammo.revision = 5;
    ammo.damage_type = 2;
    ammo.damage = 5.;
    source.launcher = Some(launcher);
    source.ammunition = Some(ammo);
    source.missile = Some(PhysicalMissileSpec {
        ammunition_count: 1,
        speed: 20.,
        radius: 0.1,
        gravity: false,
        tracking: false,
        attack_motion: 0x4000001e,
        launch_seconds: 0.,
        duration_seconds: 0.5,
        damage_modifier: 1.,
    });
    let mut defender = prepared(true);
    defender.pk = PkStatus::Pk;
    defender.skills.push((
        7,
        PhysicalSkill {
            advancement: 2,
            current: 100000,
        },
    ));
    defender.skills.sort_by_key(|(id, _)| *id);
    combat
        .register_physical(EntityId(1), Arc::new(source), &world)
        .unwrap();
    combat
        .register_physical(EntityId(2), Arc::new(defender), &world)
        .unwrap();
    combat.supply_missile_id(EntityId(1000), &world).unwrap();
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(4), 0.)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMissile {
                target: EntityId(2),
                height: 2,
                accuracy: 0.5,
            },
            0.,
        )
        .unwrap();
    combat.step_internal(&mut world, 0.1, None);
    let launch = combat.take_launch().unwrap();
    combat
        .confirm_launch(
            PhysicalLaunchReceipt {
                operation: launch.operation,
                ammunition: 202,
                before_revision: 5,
                after_revision: 6,
                remaining: 0,
            },
            &mut world,
        )
        .unwrap();
    for i in 2..=10 {
        combat.step_internal(&mut world, f64::from(i) / 10., None);
    }
    assert!(combat.physical_events.iter().any(
        |event| matches!(event,physical::PhysicalCombatEvent::Impact{impact,..} if impact.evaded)
    ));
    for id in [1, 2] {
        assert!(
            !world
                .combatant(EntityId(id))
                .unwrap()
                .pk_activity_active(1.)
        );
    }
}
#[test]
fn retained_impact_freezes_names_and_actual_clamped_damage_at_the_write() {
    let (mut combat, mut world) = fixture();
    for (id, name) in [(1, "original attacker"), (2, "original target")] {
        let mut properties = bace_entity::EntityProperties::new(8).unwrap();
        let change = properties
            .propose(
                bace_entity::PropertyFamily::String,
                1,
                Some(bace_entity::PropertyValue::String(name.into())),
            )
            .unwrap();
        properties.adopt(change).unwrap();
        world.register_properties(EntityId(id), properties).unwrap();
    }
    combat
        .register_physical(EntityId(1), Arc::new(prepared(true)), &world)
        .unwrap();
    combat
        .register_physical(EntityId(2), Arc::new(prepared(false)), &world)
        .unwrap();
    world
        .combatant_mut(EntityId(2))
        .unwrap()
        .damage(95)
        .unwrap();
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(2), 0.)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5,
            },
            0.,
        )
        .unwrap();
    combat.step_internal(&mut world, 0.1, None);
    let event = combat
        .physical_events
        .iter()
        .find(|e| matches!(e, physical::PhysicalCombatEvent::Impact { .. }))
        .unwrap()
        .clone();
    let properties = world.properties_mut(EntityId(2)).unwrap();
    let change = properties
        .propose(
            bace_entity::PropertyFamily::String,
            1,
            Some(bace_entity::PropertyValue::String("later target".into())),
        )
        .unwrap();
    properties.adopt(change).unwrap();
    let physical::PhysicalCombatEvent::Impact {
        attacker_name,
        target_name,
        applied,
        current,
        maximum,
        impact,
        ..
    } = event
    else {
        panic!()
    };
    assert_eq!(attacker_name.as_ref(), "original attacker");
    assert_eq!(target_name.as_ref(), "original target");
    assert!(impact.damage >= 5);
    assert_eq!((applied, current, maximum), (5, 0, 100));
}

#[path = "fault_tests.rs"]
mod faults;
#[path = "proc_tests.rs"]
mod procs;
