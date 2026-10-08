use super::*;
fn entry(spell: u32) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster: 1,
        school: MagicSchool::Item,
        spec: EnchantmentSpec {
            category: spell as u16,
            power: 10,
            duration: 30.,
            layer: 1,
            stat_type: 0x2008004,
            stat_key: 1,
            value: 1.,
            beneficial: true,
            set_id: None,
        },
        start_time: 0.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    }
}
#[test]
fn empty_restore_keeps_logical_limit_without_preallocating_unused_rows() {
    let mut registry = EnchantmentRegistry::restore(4096, 7, vec![]).unwrap();
    assert_eq!(registry.capacity(), 4096);
    assert_eq!(registry.entries.capacity(), 0);
    registry.add(entry(1), 0., false).unwrap();
    assert_eq!(registry.revision(), 8);
    assert_eq!(registry.entries().len(), 1);
    assert!(registry.entries.capacity() < 4096);
}
#[test]
fn lazy_storage_preserves_gameplay_capacity_rejection() {
    let mut registry = EnchantmentRegistry::restore(1, 0, vec![]).unwrap();
    registry.add(entry(1), 0., false).unwrap();
    assert_eq!(
        registry.add(entry(2), 0., false),
        Err(RegistryError::Capacity)
    );
    assert_eq!(registry.entries().len(), 1);
}
