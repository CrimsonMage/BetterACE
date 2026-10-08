use super::periodic_tests::{entry, magic, world};
use super::*;
#[test]
fn gdle_native_periodic_aggregates_categories_and_preserves_absent_source() {
    let mut m = magic(
        8,
        vec![
            entry(5939, 685, 318, 0x9004, 10., 5.),
            entry(5208, 631, 318, 0x9004, 15., 5.),
        ],
    );
    let mut profile = bace_magic::MagicDamageProfile::neutral(true);
    profile.magic_defense = bace_gameplay_api::weapon_combat::PhysicalSkill {
        advancement: 3,
        current: 600,
    };
    m.register_damage_profile(EntityId(1), profile).unwrap();
    m.damage_spell_flags.insert(5939, 0x10000);
    let mut w = world(false, 100);
    m.heartbeat(5.);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(
        w.vital(EntityId(1), EntityVital::Health).unwrap().current,
        75
    );
    assert!(matches!(
        m.combat.pop_front(),
        Some(CombatEvent::Damage {
            attacker: None,
            amount: 25,
            ..
        })
    ));
    assert!(m.registry(EntityId(1)).unwrap().entries().is_empty());
}
#[test]
fn native_hot_repeats_source_aggregate_and_backpressure_retains_final_expiry() {
    let mut m = magic(
        1,
        vec![
            entry(100, 617, 312, 0x9004, 5., 5.),
            entry(101, 630, 312, 0x9004, 7., 5.),
        ],
    );
    let mut profile = bace_magic::MagicDamageProfile::neutral(true);
    profile.healing_ratings = [100, 1000, 0];
    m.register_damage_profile(EntityId(1), profile).unwrap();
    let mut w = world(false, 50);
    m.heartbeat(5.);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(
        w.vital(EntityId(1), EntityVital::Health).unwrap().current,
        62
    );
    m.take_event();
    for _ in 0..3 {
        m.drain_periodic(&mut w, &Combat::new(64));
    }
    assert_eq!(
        w.vital(EntityId(1), EntityVital::Health).unwrap().current,
        74
    );
    m.take_event();
    for _ in 0..3 {
        m.drain_periodic(&mut w, &Combat::new(64));
    }
    assert!(matches!(
        m.take_event(),
        Some(MagicEvent::EnchantmentsRemoved { .. })
    ));
}
#[test]
fn native_nether_uses_live_dot_rating_and_player_modifier() {
    let mut m = magic(
        8,
        vec![
            entry(100, 636, 330, 0x9004, 50., 5.),
            entry(101, 12, 350, 0x9004, 50., 20.),
        ],
    );
    let mut profile = bace_magic::MagicDamageProfile::neutral(true);
    profile.dot_ratings = [50, 0];
    m.register_damage_profile(EntityId(1), profile).unwrap();
    m.register_periodic_defense(
        EntityId(1),
        PeriodicDefenseProfile {
            augmentation_reduction: 0,
            dot_resistance: 0,
            void_player_modifier: 0.5,
        },
    )
    .unwrap();
    let mut w = world(false, 100);
    m.heartbeat(5.);
    m.drain_periodic(&mut w, &Combat::new(64));
    assert_eq!(
        w.vital(EntityId(1), EntityVital::Health).unwrap().current,
        88
    );
}
