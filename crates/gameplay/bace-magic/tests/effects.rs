use bace_magic::{
    EffectError, EnchantmentEntry, EnchantmentRegistry, EnchantmentSpec, EnchantmentStack,
    MagicSchool, VitalState, boost, transfer,
};
fn entry(spell: u32, caster: u32, power: u32, duration: f64) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster,
        school: MagicSchool::Creature,
        spec: EnchantmentSpec {
            category: 1,
            power,
            duration,
            layer: 0,
            stat_type: 0x8001,
            stat_key: 1,
            value: 10.0,
            beneficial: true,
            set_id: None,
        },
        start_time: 0.0,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    }
}
#[test]
fn boost_and_transfer_clamp_vitals_and_preserve_bankers_rounding() {
    let state = VitalState {
        current: 5,
        maximum: 10,
    };
    assert_eq!(boost(state, 1, 1, 1, 2.5).unwrap().after, 7);
    assert_eq!(boost(state, -20, -20, -20, 1.0).unwrap().after, 0);
    assert_eq!(boost(state, 0, 1, 2, 1.0), Err(EffectError::InvalidRoll));
    let change = transfer(
        VitalState {
            current: 100,
            maximum: 100,
        },
        VitalState {
            current: 95,
            maximum: 100,
        },
        0.5,
        0.1,
        0,
        1.0,
        1.0,
    )
    .unwrap();
    assert_eq!(change.destination.after, 100);
    assert_eq!(change.source.after, 94);
}
#[test]
fn enchantments_refresh_only_matching_casters_and_heartbeat_time_is_paused_offline() {
    let mut registry = EnchantmentRegistry::new(8).unwrap();
    let first = registry
        .add(entry(1, 10, 100, 30.0), 1000.0, false)
        .unwrap();
    assert_eq!(first.stack, EnchantmentStack::Initial);
    let other = registry
        .add(entry(1, 20, 100, 30.0), 1000.0, false)
        .unwrap();
    assert_eq!(other.entry.spec.layer, 2);
    let mut expired = Vec::with_capacity(8);
    registry.heartbeat(5.0, &mut expired).unwrap();
    assert_eq!(registry.entries()[0].start_time, -5.0);
    let refreshed = registry
        .add(entry(1, 10, 100, 30.0), 2000.0, false)
        .unwrap();
    assert_eq!(refreshed.entry.spec.layer, 1);
    assert_eq!(refreshed.entry.start_time, 0.0);
    assert_eq!(registry.entries()[1].start_time, -5.0);
    for _ in 0..5 {
        registry.heartbeat(5.0, &mut expired).unwrap();
    }
    assert_eq!(expired, [(1, 2)]);
    registry.heartbeat(5.0, &mut expired).unwrap();
    assert_eq!(expired, [(1, 2), (1, 1)]);
}
#[test]
fn registry_output_backpressure_and_proposals_preserve_all_state() {
    let mut registry = EnchantmentRegistry::new(1).unwrap();
    let proposal = registry
        .propose_add(entry(1, 10, 100, 5.0), 0.0, false)
        .unwrap();
    assert!(registry.entries().is_empty());
    registry.adopt(proposal).unwrap();
    let revision = registry.revision();
    assert!(registry.add(entry(2, 10, 200, 5.0), 0.0, false).is_err());
    assert_eq!(registry.revision(), revision);
    let mut out = Vec::new();
    assert!(registry.heartbeat(5.0, &mut out).is_err());
    assert_eq!(registry.entries()[0].start_time, 0.0);
}
