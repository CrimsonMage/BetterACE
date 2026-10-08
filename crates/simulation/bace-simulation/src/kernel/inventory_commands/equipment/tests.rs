use super::*;

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
