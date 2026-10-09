use super::*;
use crate::inventory::Inventory;
use bace_inventory::{InventoryContainer, InventoryItem};

// Pinned ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b,
// Player_Inventory.cs SHA256 ef81aea7c07860e46ad92b93369407fa0f110591778776e7e4a6ee3f931d1230,
// TryShuffleStance and RequiresStanceSwap (lines 252-290, 438-480).
fn slot(location: u32, style: Option<u32>) -> bace_inventory::WieldSlotItem {
    bace_inventory::WieldSlotItem {
        item: EntityId(location.max(1)),
        revision: 1,
        location,
        valid: location,
        clothing: false,
        coverage: None,
        item_type: 1,
        style,
        weapon_skill: None,
        ammo: None,
        combat_use: None,
    }
}

#[test]
fn ace_selectable_equipment_mode_cases_include_thrown_and_missile_ammo() {
    let item = EntityId(0x8000_0001);
    let equip = |location| InventoryRequest::Equip { item, location };
    let dequip = InventoryRequest::Move {
        item,
        container: EntityId(1),
        placement: 0,
    };
    assert_eq!(equipment_mode_after(equip(0x400000), 0, 1, &[]), None);
    assert_eq!(equipment_mode_after(equip(0x400000), 0, 2, &[]), Some(4));
    assert_eq!(equipment_mode_after(equip(0x1000000), 0, 2, &[]), Some(8));
    assert_eq!(
        equipment_mode_after(equip(0x200000), 0, 2, &[slot(0x100000, Some(0x80))]),
        Some(4)
    );
    assert_eq!(
        equipment_mode_after(equip(0x200000), 0, 4, &[slot(0x100000, Some(2))]),
        Some(2)
    );
    assert_eq!(equipment_mode_after(equip(0x200), 0, 4, &[]), None);
    assert_eq!(equipment_mode_after(dequip, 0x800000, 4, &[]), Some(1));
    assert_eq!(equipment_mode_after(dequip, 0x800000, 2, &[]), None);
    assert_eq!(
        equipment_mode_after(dequip, 0x200000, 2, &[slot(0x100000, Some(0x80))]),
        Some(4)
    );
    assert_eq!(equipment_mode_after(dequip, 0x1000000, 8, &[]), Some(2));
}

#[test]
fn dequip_stance_uses_only_the_complete_revision_fenced_equipped_roster() {
    let actor = EntityId(1);
    let mut inventory = Inventory::new(8);
    inventory
        .register_container(InventoryContainer {
            id: actor,
            revision: 1,
            root_owner: Some(actor),
            slots: 8,
            pack_slots: 0,
            burden_limit: 100,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    let equipped = |location| InventoryItem {
        id: EntityId(location),
        revision: 7,
        template: 100,
        structure: None,
        stack_key: 100,
        place: ItemPlace::Contained {
            container: actor,
            slot: 0,
            equipped: location,
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
        valid_wield: location,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    inventory.register_item(equipped(0x100000)).unwrap();
    inventory.register_item(equipped(0x200000)).unwrap();
    let side_container = EntityId(0x8000_0003);
    inventory
        .register_container(InventoryContainer {
            id: side_container,
            revision: 1,
            root_owner: Some(actor),
            slots: 8,
            pack_slots: 0,
            burden_limit: 100,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    let mut nested = equipped(0x400000);
    nested.id = EntityId(0x8000_0004);
    nested.place = ItemPlace::Contained {
        container: side_container,
        slot: 0,
        equipped: 0x400000,
    };
    inventory.register_item(nested).unwrap();
    let source = EntityId(0x200000);
    let mut weapon = slot(0x100000, Some(0x80));
    weapon.revision = 7;
    let mut shield = slot(0x200000, None);
    shield.revision = 7;
    let roster = [weapon.clone(), shield.clone()];
    assert_eq!(
        validate_dequip_slot_roster(&inventory, actor, source, &roster),
        Ok(())
    );
    assert_eq!(
        equipment_mode_after(
            InventoryRequest::Move {
                item: source,
                container: actor,
                placement: 0,
            },
            0x200000,
            2,
            &roster,
        ),
        Some(4)
    );
    assert_eq!(
        validate_dequip_slot_roster(&inventory, actor, source, &[shield.clone()]),
        Err(E::InvalidState),
        "omitting the thrown weapon would misselect the post-dequip stance"
    );
    weapon.revision -= 1;
    assert_eq!(
        validate_dequip_slot_roster(&inventory, actor, source, &[weapon, shield.clone()]),
        Err(E::InvalidState)
    );
    let mut wrong_location = shield.clone();
    wrong_location.location = 0x400000;
    assert_eq!(
        validate_dequip_slot_roster(
            &inventory,
            actor,
            source,
            &[roster[0].clone(), wrong_location]
        ),
        Err(E::InvalidState)
    );
    assert_eq!(
        validate_dequip_slot_roster(&inventory, actor, source, &[shield.clone(), shield]),
        Err(E::InvalidState)
    );
    let mut forged_nested = slot(0x400000, None);
    forged_nested.item = EntityId(0x8000_0004);
    forged_nested.revision = 7;
    assert!(inventory.owned(actor, forged_nested.item));
    assert_eq!(
        validate_dequip_slot_roster(
            &inventory,
            actor,
            source,
            &[roster[0].clone(), roster[1].clone(), forged_nested]
        ),
        Err(E::InvalidState)
    );
}
