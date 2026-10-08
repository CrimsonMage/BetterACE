use super::*;
use bace_combat::preparation::{
    PhysicalEquipmentSource, PhysicalQualityFamily, PhysicalQualityProjection,
    PreparedPhysicalRefreshSource,
};
use bace_content::{BodyPart, Property, WeenieV1};
fn raw_source(k: &Kernel, buffed: u32) -> PreparedPhysicalRefreshSource {
    let profile = k.physical_combat_profile(EntityId(1)).unwrap().clone();
    let mut actor = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "physical-registry-owner".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    actor.properties.body_parts = vec![Property {
        id: 0,
        value: BodyPart {
            d_type: 4,
            d_val: 3,
            d_var: 0.2,
            llf: 1.,
            llb: 1.,
            lrf: 1.,
            lrb: 1.,
            mlf: 1.,
            mlb: 1.,
            mrf: 1.,
            mrb: 1.,
            hlf: 1.,
            hlb: 1.,
            hrf: 1.,
            hrb: 1.,
            ..Default::default()
        },
    }];
    let equipment = profile
        .equipment
        .iter()
        .map(|e| {
            let weapon = [&profile.main, &profile.launcher, &profile.ammunition]
                .into_iter()
                .flatten()
                .find(|w| w.entity == e.entity)
                .unwrap();
            let damage = if e.location == 0x800000 {
                weapon.damage - profile.launcher.as_ref().unwrap().damage
            } else {
                weapon.damage
            };
            let mut source = WeenieV1 {
                schema_version: 1,
                weenie_id: e.entity,
                class_name: format!("item-{}", e.entity),
                weenie_type: 6,
                last_modified: None,
                properties: Default::default(),
            };
            source.properties.ints = vec![
                Property {
                    id: 44,
                    value: damage as i32,
                },
                Property {
                    id: 45,
                    value: weapon.damage_type as i32,
                },
                Property {
                    id: 46,
                    value: if e.location == 0x400000 { 0x10 } else { 4 },
                },
                Property {
                    id: 47,
                    value: weapon.attack_type as i32,
                },
                Property {
                    id: 48,
                    value: weapon.skill as i32,
                },
            ];
            source.properties.floats = vec![Property {
                id: 62,
                value: weapon.offense,
            }];
            PhysicalEquipmentSource {
                entity: e.entity,
                revision: e.revision,
                location: e.location,
                weenie: Arc::new(source),
            }
        })
        .collect();
    PreparedPhysicalRefreshSource {
        actor: 1,
        weenie: Arc::new(actor),
        equipment,
        qualities: vec![PhysicalQualityProjection {
            entity: buffed,
            family: PhysicalQualityFamily::Int,
            stat: 44,
            part: None,
            details: quality(20.0),
            value: 20.0,
        }],
        profile,
    }
}
fn buff(duration: f64) -> bace_magic::EnchantmentRegistry {
    bace_magic::EnchantmentRegistry::restore(
        16,
        1,
        vec![bace_magic::EnchantmentEntry {
            spell: 123,
            caster: 1,
            school: bace_magic::MagicSchool::Item,
            spec: bace_magic::EnchantmentSpec {
                category: 23,
                power: 100,
                duration,
                layer: 1,
                stat_type: 0x9004,
                stat_key: 44,
                value: 10.0,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: Default::default(),
        }],
    )
    .unwrap()
}
#[test]
fn equipped_buff_expiry_rebinds_exact_item_revision_and_updates_active_melee() {
    let mut damages = vec![];
    for duration in [1.0, -1.0] {
        let mut k = connected(1.5);
        k.register_magic_registry(EntityId(100), buff(duration), true)
            .unwrap();
        let source = Arc::new(raw_source(&k, 100));
        k.register_physical_refresh_source(EntityId(1), source.clone())
            .unwrap();
        k.step().unwrap();
        drain(&mut k);
        assert_eq!(
            k.physical_combat_profile(EntityId(1))
                .unwrap()
                .main
                .as_ref()
                .unwrap()
                .damage,
            30.0
        );
        while k.ticks() < 145 {
            k.step().unwrap();
            drain(&mut k);
        }
        k.enqueue(attack(2)).unwrap();
        let mut impact = None;
        for _ in 0..20 {
            k.step().unwrap();
            for e in drain(&mut k) {
                if let bace_simulation::PhysicalCombatEvent::Impact { impact: v, .. } = e {
                    impact = Some(v);
                }
            }
        }
        damages.push(
            impact
                .expect("trusted hook after registry heartbeat")
                .damage,
        );
        let p = k.physical_combat_profile(EntityId(1)).unwrap();
        assert_eq!(
            p.main.as_ref().unwrap().damage,
            if duration > 0.0 { 20.0 } else { 30.0 }
        );
        assert_eq!(
            p.equipment[0].revision, 2,
            "one owner heartbeat revision, including permanent-row aging"
        );
        if duration > 0.0 {
            assert!(
                k.register_physical_refresh_source(EntityId(1), source)
                    .is_err(),
                "stale raw source cannot replace acknowledged revision"
            );
        }
        for _ in 0..10 {
            k.step().unwrap();
            drain(&mut k);
        }
        assert_eq!(
            k.physical_combat_profile(EntityId(1))
                .unwrap()
                .main
                .as_ref()
                .unwrap()
                .damage,
            if duration > 0.0 { 20.0 } else { 30.0 },
            "refresh must never compound"
        );
    }
    assert!(
        damages[0] < damages[1],
        "expiry must change the pending melee impact: {damages:?}"
    );
}
#[test]
fn launcher_expiry_updates_future_profiles_but_keeps_the_launched_damage_snapshot() {
    let mut damages = vec![];
    for duration in [1.0, -1.0] {
        let mut k = missile_kernel();
        // Move the target through accepted input so flight straddles expiration.
        let epoch = k.world().body(EntityId(2)).unwrap().accepted().epoch();
        k.enqueue(Command::Movement {
            actor: EntityId(2),
            epoch,
            sequence: 1,
            intent: bace_motion::MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
        })
        .unwrap();
        for _ in 0..65 {
            k.step().unwrap();
            drain(&mut k);
        }
        k.enqueue(Command::Movement {
            actor: EntityId(2),
            epoch,
            sequence: 2,
            intent: bace_motion::MotionIntent::new(Vec3::ZERO, false).unwrap(),
        })
        .unwrap();
        k.step().unwrap();
        drain(&mut k);
        let mut p = (**k.physical_combat_profile(EntityId(1)).unwrap()).clone();
        p.style = 0x8000003f;
        p.launcher.as_mut().unwrap().style = 0x10;
        p.ammunition.as_mut().unwrap().damage += p.launcher.as_ref().unwrap().damage;
        let speed = bace_combat::physical::missile_attack_speed(
            p.quickness,
            p.launcher.as_ref().unwrap().attack_time,
        )
        .unwrap();
        k.register_physical_combat(EntityId(1), Arc::new(p))
            .unwrap();
        for motion in [0x4000001e, 0x40000016] {
            k.register_physical_motion(
                EntityId(1),
                motion,
                speed,
                chain_in_style(motion, speed, 16, 9, 0x8000003f),
            )
            .unwrap();
        }
        k.set_physical_options(EntityId(1), 0, 0).unwrap();
        k.register_magic_registry(EntityId(101), buff(duration), true)
            .unwrap();
        k.register_physical_refresh_source(EntityId(1), Arc::new(raw_source(&k, 101)))
            .unwrap();
        k.step().unwrap();
        drain(&mut k);
        let expiry = k.ticks() + 150;
        while k.ticks() < expiry - 25 {
            k.step().unwrap();
            drain(&mut k);
        }
        k.enqueue(combat(
            3,
            CombatRequest::TargetedMissile {
                target: EntityId(2),
                height: 2,
                accuracy: 0.5,
            },
        ))
        .unwrap();
        let mut launch = None;
        for _ in 0..24 {
            k.step().unwrap();
            drain(&mut k);
            if let Some(p) = k.take_physical_launch() {
                launch = Some(p);
                break;
            }
        }
        let launch = launch.expect("trusted aim");
        receipt(&mut k, &launch);
        let mut impact = None;
        for _ in 0..50 {
            k.step().unwrap();
            for e in drain(&mut k) {
                if let bace_simulation::PhysicalCombatEvent::Impact { impact: v, .. } = e {
                    impact = Some(v);
                }
            }
        }
        damages.push(impact.expect("flight impact after expiry").damage);
        let p = k.physical_combat_profile(EntityId(1)).unwrap();
        assert_eq!(
            p.launcher.as_ref().unwrap().damage,
            if duration > 0.0 { 20.0 } else { 30.0 }
        );
        assert_eq!(
            p.ammunition.as_ref().unwrap().damage,
            if duration > 0.0 { 25.0 } else { 35.0 }
        );
    }
    assert_eq!(
        damages[0], damages[1],
        "launched immutable equipment snapshot survives later registry expiry"
    );
}
