use super::*;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
fn kernel() -> Kernel {
    let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(1),
            revision: 1,
            root_owner: None,
            slots: 10,
            pack_slots: 10,
            burden_limit: 10000,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    for id in [2, 3] {
        kernel
            .register_inventory_item(InventoryItem {
                structure: None,
                id: EntityId(id),
                revision: 1,
                template: 100,
                stack_key: 0,
                place: ItemPlace::Contained {
                    container: EntityId(1),
                    slot: 0,
                    equipped: if id == 2 { 1 } else { 2 },
                },
                stack: 1,
                maximum_stack: 1,
                unit_burden: 0,
                unit_value: 0,
                pack_slot: false,
                is_container: false,
                attuned: false,
                trade_reserved: false,
                active_pet: false,
                unique: false,
                quest_allowed: true,
                valid_wield: 3,
                incompatible_wield: 0,
                wield_requirements_met: true,
            })
            .unwrap();
    }
    kernel
}
fn enchantment(item: u32, spell: u32) -> PreparedGeneratorEnchantment {
    PreparedGeneratorEnchantment {
        target: EntityId(1),
        entry: bace_magic::EnchantmentEntry {
            spell,
            caster: item,
            school: bace_magic::MagicSchool::Creature,
            spec: bace_magic::EnchantmentSpec {
                category: spell as u16,
                power: 100,
                duration: 30.0,
                layer: 0,
                stat_type: 1,
                stat_key: 1,
                value: 10.0,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: Default::default(),
        },
    }
}
#[test]
fn source_delay_uses_three_ticks_and_permanent_item_caster_entries() {
    let mut kernel = kernel();
    let entries = vec![enchantment(2, 10), enchantment(3, 11)];
    kernel
        .preflight_generated_enchantments(EntityId(1), &entries)
        .unwrap();
    kernel.enqueue_generated_enchantments(EntityId(1), entries);
    for tick in [1, 2] {
        kernel.step_generated_enchantments(tick).unwrap();
        assert!(kernel.magic_registry(EntityId(1)).is_none());
        assert!(kernel.take_magic_event().is_none());
    }
    kernel.step_generated_enchantments(3).unwrap();
    let registry = kernel.magic_registry(EntityId(1)).unwrap();
    assert_eq!(registry.entries().len(), 2);
    assert!(
        registry
            .entries()
            .iter()
            .all(|entry| entry.spec.duration == -1.0 && entry.start_time == 0.0)
    );
    assert_eq!(
        registry
            .entries()
            .iter()
            .map(|entry| entry.caster)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert!(!kernel.generated_enchantments.has_state());
    assert!(kernel.take_magic_event().is_some());
    assert!(kernel.take_magic_event().is_some());
    assert!(kernel.take_magic_event().is_none());
    kernel.step_generated_enchantments(4).unwrap();
    assert_eq!(kernel.magic_registry(EntityId(1)).unwrap().revision(), 2);
}
#[test]
fn reserved_registry_keeps_exact_batch_until_owner_releases_it() {
    let mut kernel = kernel();
    kernel
        .register_magic_registry(
            EntityId(1),
            bace_magic::EnchantmentRegistry::new(10).unwrap(),
            true,
        )
        .unwrap();
    let entries = vec![enchantment(2, 10), enchantment(3, 11)];
    kernel
        .preflight_generated_enchantments(EntityId(1), &entries)
        .unwrap();
    kernel.enqueue_generated_enchantments(EntityId(1), entries.clone());
    kernel.reserve_magic_registry(EntityId(1), true).unwrap();
    kernel.step_generated_enchantments(3).unwrap();
    assert!(
        kernel
            .magic_registry(EntityId(1))
            .unwrap()
            .entries()
            .is_empty()
    );
    assert_eq!(
        kernel.generated_enchantments.pending[&EntityId(1)].entries,
        entries
    );
    assert!(kernel.take_magic_event().is_none());
    kernel.reserve_magic_registry(EntityId(1), false).unwrap();
    kernel.step_generated_enchantments(4).unwrap();
    assert_eq!(
        kernel.magic_registry(EntityId(1)).unwrap().entries().len(),
        2
    );
    assert!(!kernel.generated_enchantments.has_state());
}
#[test]
fn invalid_spell_batch_rejected_before_admission_and_missing_item_does_not_cancel_other_item() {
    let mut kernel = kernel();
    let mut invalid = enchantment(2, 10);
    invalid.entry.spec.value = f32::NAN;
    assert!(
        kernel
            .preflight_generated_enchantments(EntityId(1), &[invalid])
            .is_err()
    );
    assert!(!kernel.generated_enchantments.has_state());
    let entries = vec![enchantment(2, 10), enchantment(99, 11)];
    kernel
        .preflight_generated_enchantments(EntityId(1), &entries)
        .unwrap();
    kernel.enqueue_generated_enchantments(EntityId(1), entries);
    kernel.step_generated_enchantments(3).unwrap();
    assert_eq!(
        kernel.magic_registry(EntityId(1)).unwrap().entries().len(),
        1
    );
    assert_eq!(
        kernel.magic_registry(EntityId(1)).unwrap().entries()[0].caster,
        2
    );
}
#[test]
fn registry_capacity_failure_never_partially_applies_first_equipped_spell() {
    let mut kernel = kernel();
    kernel
        .register_magic_registry(
            EntityId(1),
            bace_magic::EnchantmentRegistry::new(1).unwrap(),
            true,
        )
        .unwrap();
    let entries = vec![enchantment(2, 10), enchantment(3, 11)];
    kernel
        .preflight_generated_enchantments(EntityId(1), &entries)
        .unwrap();
    kernel.enqueue_generated_enchantments(EntityId(1), entries);
    kernel.step_generated_enchantments(3).unwrap();
    assert!(
        kernel
            .magic_registry(EntityId(1))
            .unwrap()
            .entries()
            .is_empty()
    );
    assert!(kernel.take_magic_event().is_none());
    assert!(kernel.generated_enchantments.has_state());
    kernel.cancel_generated_enchantments(EntityId(1));
    assert!(!kernel.generated_enchantments.has_state());
    assert_eq!(kernel.generated_enchantments.count, 0);
}

#[test]
fn empty_generated_creature_cleanup_retires_wearer_registry_only_after_reservation_release() {
    let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(1),
            revision: 1,
            root_owner: None,
            slots: 10,
            pack_slots: 10,
            burden_limit: 1000,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    let mut registry = bace_magic::EnchantmentRegistry::new(8).unwrap();
    registry.add(enchantment(2, 10).entry, 0.0, true).unwrap();
    kernel
        .register_magic_registry(EntityId(1), registry, true)
        .unwrap();
    kernel.reserve_magic_registry(EntityId(1), true).unwrap();
    assert!(
        kernel
            .retire_generated_creature_equipment(EntityId(1))
            .is_err()
    );
    assert!(kernel.magic_registry(EntityId(1)).is_some());
    assert!(kernel.inventory_container(EntityId(1)).is_some());
    kernel.reserve_magic_registry(EntityId(1), false).unwrap();
    kernel
        .retire_generated_creature_equipment(EntityId(1))
        .unwrap();
    assert!(kernel.magic_registry(EntityId(1)).is_none());
    assert!(kernel.inventory_container(EntityId(1)).is_none());
}
