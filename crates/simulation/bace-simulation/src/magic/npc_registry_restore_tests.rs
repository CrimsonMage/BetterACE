use super::*;
fn saved_entry() -> EnchantmentEntry {
    EnchantmentEntry {
        spell: 500,
        caster: 77,
        school: bace_magic::MagicSchool::Creature,
        spec: bace_magic::EnchantmentSpec {
            category: 8,
            power: 100,
            duration: 12.5,
            layer: 2,
            stat_type: 0x1000,
            stat_key: 1,
            value: 7.,
            beneficial: true,
            set_id: None,
        },
        start_time: -1.25,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: EnchantmentMetadata {
            enchantment_category: 1,
            last_time_degraded: -1.,
            ..Default::default()
        },
    }
}
#[test]
fn fresh_object_restore_preserves_exact_registry_and_refuses_overwrite() {
    let (mut magic, mut world, _) = super::object_caster_tests::fixture();
    world
        .register_properties(EntityId(2), bace_entity::EntityProperties::new(16).unwrap())
        .unwrap();
    let entry = saved_entry();
    magic
        .restore_object_registry(EntityId(2), 17, vec![entry.clone()], &world, 0.)
        .unwrap();
    assert_eq!(magic.registry(EntityId(2)).unwrap().revision(), 17);
    assert_eq!(magic.registry(EntityId(2)).unwrap().entries(), &[entry]);
    assert!(
        magic.events.is_empty(),
        "restore does not replay initial enchantment outputs"
    );
    assert_eq!(
        magic.restore_object_registry(EntityId(2), 18, vec![], &world, 0.),
        Err(CastRejection::Busy)
    );
    assert_eq!(magic.registry(EntityId(2)).unwrap().revision(), 17);
    assert!(world.combatant(EntityId(2)).is_none());
}
#[test]
fn malformed_or_queued_restore_preserves_initial_owner() {
    let (mut magic, mut world, _) = super::object_caster_tests::fixture();
    world
        .register_properties(EntityId(2), bace_entity::EntityProperties::new(16).unwrap())
        .unwrap();
    let mut entry = saved_entry();
    entry.spec.value = f32::NAN;
    assert!(
        magic
            .restore_object_registry(EntityId(2), 5, vec![entry], &world, 0.)
            .is_err()
    );
    assert_eq!(magic.registry(EntityId(2)).unwrap().revision(), 0);
    magic.events.push_back(MagicEvent::Fizzle {
        movement_incarnation: None,
        actor: EntityId(2),
        intensity: 1.,
    });
    assert_eq!(
        magic.restore_object_registry(EntityId(2), 5, vec![saved_entry()], &world, 0.),
        Err(CastRejection::Busy)
    );
    assert!(magic.registry(EntityId(2)).unwrap().entries().is_empty());
}
