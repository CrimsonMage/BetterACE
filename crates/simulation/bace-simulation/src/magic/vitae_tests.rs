use super::*;
fn template() -> EnchantmentEntry {
    EnchantmentEntry {
        spell: 666,
        caster: 1,
        school: bace_magic::MagicSchool::Life,
        spec: bace_magic::EnchantmentSpec {
            category: 204,
            power: 0,
            duration: -1.0,
            layer: 1,
            stat_type: 0,
            stat_key: 0,
            value: 0.95,
            beneficial: false,
            set_id: None,
        },
        start_time: 0.0,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: EnchantmentMetadata::default(),
    }
}
fn magic() -> Magic {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), EnchantmentRegistry::new(8).unwrap(), true, 0.0)
        .unwrap();
    magic.register_vitae_template(template()).unwrap();
    magic
}
#[test]
fn vitae_snapshot_precedes_adoption_and_completion_waits_two_seconds() {
    let mut magic = magic();
    let actual = 0.99999994_f32;
    let mutation = magic
        .prepare_vitae(EntityId(1), Some(actual), Some(2.0))
        .unwrap();
    assert!(magic.registry(EntityId(1)).unwrap().entries().is_empty());
    assert_eq!(mutation.entries()[0].spec.value.to_bits(), actual.to_bits());
    assert_eq!(mutation.entries()[0].metadata.enchantment_category, 4);
    assert!(magic.adopt_vitae(EntityId(1), mutation, 0.0).is_ok());
    magic.take_event();
    magic.service_vitae_removals(1.999);
    assert_eq!(magic.registry(EntityId(1)).unwrap().entries().len(), 1);
    magic.service_vitae_removals(2.0);
    assert!(magic.registry(EntityId(1)).unwrap().entries().is_empty());
    assert!(
        matches!(magic.take_event(),Some(MagicEvent::EnchantmentsRemoved{entries,..}) if entries==[(666,0)])
    );
}
#[test]
fn newer_penalty_and_stale_proposals_cannot_be_overwritten_by_old_completion() {
    let mut magic = magic();
    let completion = magic
        .prepare_vitae(EntityId(1), Some(1.0), Some(2.0))
        .unwrap();
    let stale = magic.prepare_vitae(EntityId(1), Some(0.9), None).unwrap();
    assert!(magic.adopt_vitae(EntityId(1), completion, 0.0).is_ok());
    magic.take_event();
    assert!(magic.adopt_vitae(EntityId(1), stale, 1.0).is_err());
    let penalty = magic.prepare_vitae(EntityId(1), Some(0.95), None).unwrap();
    assert!(magic.adopt_vitae(EntityId(1), penalty, 1.0).is_ok());
    magic.take_event();
    magic.service_vitae_removals(2.0);
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].spec.value,
        0.95
    );
    assert!(magic.take_event().is_none());
}
