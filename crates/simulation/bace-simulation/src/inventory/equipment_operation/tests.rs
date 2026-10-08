use super::*;

fn source() -> InventoryItem {
    InventoryItem {
        structure: Some(12),
        id: EntityId(0x8000_0001),
        revision: 7,
        template: 900,
        stack_key: 33,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 10,
        maximum_stack: 100,
        unit_burden: 5,
        unit_value: 20,
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
    }
}

#[test]
fn fresh_split_to_wield_reserves_source_and_full_new_stack_until_exact_receipt() {
    let mut inventory = Inventory::new(8);
    inventory
        .register_container(InventoryContainer {
            id: EntityId(1),
            revision: 1,
            root_owner: Some(EntityId(1)),
            slots: 16,
            pack_slots: 0,
            burden_limit: 1000,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    let source = source();
    inventory.register_item(source.clone()).unwrap();
    let fresh = InventoryItem {
        id: EntityId(0x8000_0002),
        revision: 0,
        stack: 1,
        ..source.clone()
    };
    let operation = inventory
        .apply_equipment(
            InventoryRequest::SplitToWield {
                item: source.id,
                location: 0x100000,
                amount: 3,
            },
            InventoryAuthority {
                actor: EntityId(1),
                busy: false,
                in_range: true,
                clear_path: true,
                geometry_ready: true,
                drop_validated: false,
                source_view: None,
                destination_view: None,
                new_item: Some(fresh.id),
            },
            Some(bace_inventory::StackSplitPreparation {
                fresh: fresh.clone(),
                source_stackable: true,
                source_stuck: false,
                source_vendor: false,
                destination_corpse: false,
            }),
        )
        .unwrap();
    let ticket = inventory.pending_ticket(operation).unwrap();
    assert_eq!(ticket.proposal.changes.len(), 2);
    assert_eq!(inventory.item(source.id), Some(&source));
    assert!(inventory.item(fresh.id).is_none());
    assert!(inventory.reserved(source.id));
    assert!(inventory.reserved(fresh.id));
    let receipt = InventoryReceipt {
        operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    inventory.claim(operation).unwrap();
    let mut incomplete = receipt.clone();
    incomplete.revisions.pop();
    assert_eq!(inventory.confirm(&incomplete), Err(Error::InvalidState));
    assert_eq!(inventory.item(source.id), Some(&source));
    assert!(inventory.item(fresh.id).is_none());
    inventory.confirm(&receipt).unwrap();
    let remainder = inventory.item(source.id).unwrap();
    assert_eq!((remainder.stack, remainder.revision), (7, 8));
    let wielded = inventory.item(fresh.id).unwrap();
    assert_eq!((wielded.stack, wielded.revision), (3, 1));
    assert_eq!(wielded.structure, fresh.structure);
    assert_eq!(wielded.stack_key, fresh.stack_key);
    assert_eq!(wielded.unit_burden, fresh.unit_burden);
    assert_eq!(wielded.unit_value, fresh.unit_value);
    assert!(
        matches!(wielded.place, ItemPlace::Contained { container, equipped: 0x100000, .. } if container == EntityId(1))
    );
    assert_eq!(
        inventory
            .equipped_items(EntityId(1))
            .map(|i| i.id)
            .collect::<Vec<_>>(),
        vec![fresh.id]
    );
    assert!(!inventory.reserved(source.id));
    assert!(!inventory.reserved(fresh.id));
}
