use super::*;
use bace_inventory::{InventoryItem, ItemChange, ItemPlace};

fn item(id: u32, stack: u32, equipped: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 100,
        stack_key: 100,
        place: ItemPlace::Contained {
            container: EntityId(0x5000_0001),
            slot: 0,
            equipped,
        },
        stack,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0x200000,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}

#[test]
fn split_to_wield_publication_selects_only_the_exact_fresh_proposal_row() {
    let source = EntityId(0x8000_0001);
    let fresh = EntityId(0x8000_0002);
    let request = bace_gameplay_api::InventoryRequest::SplitToWield {
        item: source,
        location: 0x200000,
        amount: 2,
    };
    let before = item(source.0, 10, 0);
    let changes = [
        ItemChange {
            before: Some(before.clone()),
            after: item(source.0, 8, 0),
        },
        ItemChange {
            before: None,
            after: item(fresh.0, 2, 0x200000),
        },
    ];
    assert_eq!(
        equipment_focus_item(source, request, Some(fresh), &changes).unwrap(),
        fresh
    );
    assert!(equipment_focus_item(source, request, None, &changes).is_err());
    assert!(equipment_focus_item(source, request, Some(source), &changes).is_err());
    assert!(equipment_focus_item(source, request, Some(EntityId(0x8000_0003)), &changes).is_err());
    assert!(equipment_focus_item(fresh, request, Some(fresh), &changes).is_err());
    let duplicate_fresh = [
        changes[0].clone(),
        changes[1].clone(),
        ItemChange {
            before: None,
            after: item(0x8000_0003, 1, 0x200000),
        },
    ];
    assert!(equipment_focus_item(source, request, Some(fresh), &duplicate_fresh).is_err());
}

#[test]
fn ordinary_equipment_publication_keeps_the_source_identity() {
    let source = EntityId(0x8000_0001);
    let changes = [ItemChange {
        before: Some(item(source.0, 1, 0)),
        after: item(source.0, 1, 0x200000),
    }];
    let request = bace_gameplay_api::InventoryRequest::Equip {
        item: source,
        location: 0x200000,
    };
    assert_eq!(
        equipment_focus_item(source, request, None, &changes).unwrap(),
        source
    );
    assert!(equipment_focus_item(source, request, Some(EntityId(0x8000_0002)), &changes).is_err());
    assert!(equipment_focus_item(EntityId(0x8000_0002), request, None, &changes).is_err());
    assert!(equipment_focus_item(source, request, None, &[]).is_err());
}
