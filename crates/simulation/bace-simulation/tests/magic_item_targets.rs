mod magic_common;
use bace_gameplay_api::CastRejection;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_magic::{EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec};
use magic_common::*;
fn setup(owner: EntityId, equipped: u32, resistance: u32) -> Kernel {
    let mut k = kernel(64);
    k.register_inventory_container(InventoryContainer {
        id: owner,
        revision: 1,
        root_owner: Some(owner),
        slots: 10,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(InventoryItem {
        structure: None,
        id: EntityId(1000),
        revision: 1,
        template: 700,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: owner,
            slot: 0,
            equipped,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0x100000,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
    k.register_magic_registry(
        EntityId(1000),
        EnchantmentRegistry::new(16).unwrap(),
        equipped != 0,
    )
    .unwrap();
    k.register_magic_item_type(EntityId(1000), 1).unwrap();
    k.register_magic_item_quality(EntityId(1000), resistance, false)
        .unwrap();
    let mut s = spell(
        100,
        SpellEffect::Enchantment(EnchantmentSpec {
            category: 52,
            power: 100,
            duration: 30.,
            layer: 1,
            stat_type: 0x9004,
            stat_key: 44,
            value: 12.,
            beneficial: true,
            set_id: None,
        }),
    );
    s.spell.school = MagicSchool::Item;
    k.register_magic_spell(s).unwrap();
    k.register_enchantment_metadata(100, EnchantmentMetadata::default())
        .unwrap();
    k.register_magic_target_mask(100, 1).unwrap();
    k
}
fn cast(k: &mut Kernel) {
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1000),
            spell: 100,
        },
    })
    .unwrap();
}
#[test]
fn actual_item_cast_uses_owner_geometry_and_changes_item_registry_without_world_body() {
    let mut k = setup(EntityId(1), 0, 0);
    assert!(k.world().actor_state(EntityId(1000)).is_err());
    cast(&mut k);
    for _ in 0..10 {
        step(&mut k);
    }
    let entries = k.magic_registry(EntityId(1000)).unwrap().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (
            entries[0].spell,
            entries[0].caster,
            entries[0].spec.stat_key
        ),
        (100, 1, 44)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
}
#[test]
fn foreign_unwielded_item_refuses_before_mana_and_unknown_type_mask_never_widens() {
    let mut k = setup(EntityId(2), 0, 0);
    cast(&mut k);
    step(&mut k);
    assert_eq!(
        k.take_cast_outcome().unwrap().result,
        Err(CastRejection::InvalidTarget)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    let mut k = setup(EntityId(1), 0, 0);
    k.register_magic_target_mask(100, 2).unwrap();
    cast(&mut k);
    step(&mut k);
    assert_eq!(
        k.take_cast_outcome().unwrap().result,
        Err(CastRejection::InvalidTarget)
    );
}
#[test]
fn equipped_foreign_item_can_be_beneficially_enchanted_but_item_immunity_resists() {
    let mut k = setup(EntityId(2), 0x100000, 0);
    cast(&mut k);
    for _ in 0..10 {
        step(&mut k);
    }
    assert_eq!(k.magic_registry(EntityId(1000)).unwrap().entries().len(), 1);
    let mut k = setup(EntityId(1), 0, 9999);
    cast(&mut k);
    let mut resisted = false;
    for _ in 0..10 {
        resisted |= step(&mut k).iter().any(|e| {
            matches!(
                e,
                MagicEvent::TargetRejected {
                    reason: CastRejection::Resisted,
                    ..
                }
            )
        });
    }
    assert!(resisted);
    assert!(
        k.magic_registry(EntityId(1000))
            .unwrap()
            .entries()
            .is_empty()
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
}
