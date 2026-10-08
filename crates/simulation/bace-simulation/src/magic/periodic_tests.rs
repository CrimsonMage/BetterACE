use super::*;
use bace_combat::specialization::CombatSkill;
use bace_gameplay_api::SkillAdvancement;
pub(super) fn entry(
    spell: u32,
    category: u16,
    key: u32,
    flags: u32,
    value: f32,
    duration: f64,
) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster: 2,
        school: bace_magic::MagicSchool::Creature,
        spec: bace_magic::EnchantmentSpec {
            category,
            power: 100,
            duration,
            layer: 1,
            stat_type: flags,
            stat_key: key,
            value,
            beneficial: false,
            set_id: None,
        },
        start_time: 0.0,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    }
}
pub(super) fn magic(capacity: usize, entries: Vec<EnchantmentEntry>) -> Magic {
    let mut m = Magic::new(capacity);
    m.register_registry(
        EntityId(1),
        EnchantmentRegistry::restore(16, 0, entries).unwrap(),
        true,
        0.0,
    )
    .unwrap();
    m
}
pub(super) fn world(source: bool, health: u32) -> World {
    let mut w = super::specialization_tests::world();
    if health < 100 {
        w.apply_vital_batch(
            &[VitalMutation {
                actor: EntityId(1),
                vital: EntityVital::Health,
                before: 100,
                after: health,
            }],
            None,
        )
        .unwrap();
    }
    if source {
        let cell = bace_types::CellId(1);
        let body = bace_physics::Body::spawn(
            w.scene(cell).unwrap(),
            Vec3::new(3.0, 0.0, 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        w.insert(bace_entity::Actor {
            id: EntityId(2),
            cell,
            body,
        })
        .unwrap();
        w.register_combatant(
            EntityId(2),
            bace_entity::Combatant::new(bace_entity::CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 2.0,
                attack_duration: 1.0,
                strike_offsets: vec![0.5],
                player: false,
            })
            .unwrap(),
        )
        .unwrap();
    }
    w
}
#[test]
fn original_ace_periodic_and_debuff_methods_match_supported_consumers() {
    for line in include_str!("../../../../gameplay/bace-magic/tests/fixtures/dirty.csv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let c: Vec<_> = line.split(',').collect();
        match c[0] {
            "damage" => {
                let amount = c[1].parse().unwrap();
                let rating: u32 = c[2].parse().unwrap();
                let mut w = world(c[4] == "1", c[3].parse().unwrap());
                let mut m = magic(8, vec![entry(5939, 685, 318, 0x9004, amount, 5.0)]);
                m.refresh_magic_defenses(
                    EntityId(1),
                    MagicDefenseProfile {
                        player: true,
                        magic_defense: CombatSkill {
                            advancement: SkillAdvancement::Specialized,
                            base: rating * 50,
                            current: rating * 50,
                        },
                        shield: None,
                    },
                )
                .unwrap();
                m.heartbeat(5.0);
                m.drain_periodic(&mut w, &Combat::new(64));
                assert_eq!(
                    w.vital(EntityId(1), EntityVital::Health).unwrap().current,
                    c[5].parse::<u32>().unwrap(),
                    "{line}"
                );
                assert!(m.registry(EntityId(1)).unwrap().entries().is_empty());
            }
            "debuff" => {
                let amount = c[1].parse().unwrap();
                let m = magic(
                    8,
                    vec![
                        entry(5938, 684, 0, 0x18010, amount, 20.0),
                        entry(5940, 686, 0, 0x28010, amount, 20.0),
                    ],
                );
                for skill in [33, 34, 43, 44, 49] {
                    assert_eq!(
                        m.dirty_skill_modifier(EntityId(1), skill),
                        c[2].parse().unwrap()
                    );
                }
                for skill in [6, 7, 15, 48] {
                    assert_eq!(
                        m.dirty_skill_modifier(EntityId(1), skill),
                        c[3].parse().unwrap()
                    );
                }
                for skill in [16, 21, 31, 32, 52] {
                    assert_eq!(m.dirty_skill_modifier(EntityId(1), skill), 0);
                }
            }
            "top" => {
                let first = entry(5939, 685, 318, 0x9004, 10.0, 20.0);
                let mut second = first.clone();
                second.spell = 5943;
                second.caster = 3;
                second.spec.layer = 2;
                let m = magic(8, vec![first, second]);
                assert_eq!(
                    m.registry(EntityId(1))
                        .unwrap()
                        .top(685, 0.0)
                        .unwrap()
                        .caster,
                    c[1].parse::<u32>().unwrap()
                );
            }
            "healing" => {
                let m = magic(
                    8,
                    vec![entry(5941, 687, 317, 0x9004, c[1].parse().unwrap(), 20.0)],
                );
                assert!(
                    (m.healing_amount_modifier(EntityId(1)) - c[2].parse::<f32>().unwrap()).abs()
                        < 0.000001
                );
            }
            _ => panic!("unknown oracle {line}"),
        }
    }
}
#[test]
fn full_output_preserves_each_tick_and_orders_final_damage_before_expiry() {
    let mut m = magic(1, vec![entry(5939, 685, 318, 0x9004, 10.0, 10.0)]);
    let mut w = world(true, 100);
    m.events.push_back(MagicEvent::ProjectileRemoved {
        tick: 0,
        actor: EntityId(90),
    });
    m.heartbeat(10.0);
    m.heartbeat(10.0);
    assert_eq!(
        m.registry(EntityId(1)).unwrap().entries()[0].start_time,
        -5.0
    );
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 100);
    assert!(m.can_take_registry(EntityId(1), 10.0).is_err());
    assert!(m.reserve_registry(EntityId(1), true, 10.0).is_err());
    m.take_event();
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 90);
    assert!(matches!(
        m.take_event(),
        Some(MagicEvent::Vital { after: 90, .. })
    ));
    m.take_combat_event();
    m.drain_periodic(&mut w, &Combat::new(64));
    m.heartbeat(10.0);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 80);
    assert!(matches!(
        m.take_event(),
        Some(MagicEvent::Vital { after: 80, .. })
    ));
    m.take_combat_event();
    m.drain_periodic(&mut w, &Combat::new(64));
    assert!(matches!(
        m.take_event(),
        Some(MagicEvent::EnchantmentsRemoved { .. })
    ));
    m.heartbeat(20.0);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 80);
}
#[test]
fn strongest_bleed_only_and_inactive_offline_registry_never_ticks() {
    let low = entry(5943, 685, 318, 0x9004, 4.0, 10.0);
    let mut high = entry(5939, 685, 318, 0x9004, 10.0, 10.0);
    high.spec.power = 200;
    high.spec.layer = 2;
    let mut m = magic(8, vec![low, high]);
    let mut w = world(true, 100);
    m.set_registry_active(EntityId(1), false, 0.0).unwrap();
    m.heartbeat(100.0);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 100);
    m.set_registry_active(EntityId(1), true, 100.0).unwrap();
    m.heartbeat(105.0);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 90);
}

#[test]
fn nether_categories_and_healing_share_bounded_final_tick_order() {
    let mut m = magic(
        8,
        vec![
            entry(700, 636, 330, 0x9004, 10.0, 5.0),
            entry(701, 637, 330, 0x9004, 6.0, 5.0),
            entry(702, 617, 312, 0x9004, 4.0, 5.0),
        ],
    );
    m.register_periodic_defense(
        EntityId(1),
        PeriodicDefenseProfile {
            augmentation_reduction: 0,
            dot_resistance: 100,
            void_player_modifier: 1.0,
        },
    )
    .unwrap();
    let mut w = world(true, 50);
    m.heartbeat(5.0);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(w.combatant(EntityId(1)).unwrap().health(), 46);
    let events: Vec<_> = std::iter::from_fn(|| m.take_event()).collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, MagicEvent::Vital { .. }))
            .count(),
        3
    );
    assert!(
        matches!(events.last(),Some(MagicEvent::EnchantmentsRemoved{entries,..}) if entries.len()==3)
    );
    assert!(m.registry(EntityId(1)).unwrap().entries().is_empty());
}
