use super::*;
fn container(id: u32, owner: Option<u32>) -> InventoryContainer {
    InventoryContainer {
        id: EntityId(id),
        revision: 1,
        root_owner: owner.map(EntityId),
        slots: 10,
        pack_slots: 10,
        burden_limit: 10000,
        accessible: true,
        open: true,
        generation: 1,
    }
}
fn item(id: u32, place: ItemPlace, bag: bool) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 10,
        stack_key: 0,
        place,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: bag,
        is_container: bag,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
#[test]
fn generated_bag_acquisition_reserves_and_promotes_the_complete_tree() {
    let mut inventory = Inventory::new(20);
    inventory.register_container(container(1, Some(1))).unwrap();
    let bag = item(10, ItemPlace::World, true);
    let child = item(
        11,
        ItemPlace::Contained {
            container: EntityId(10),
            slot: 0,
            equipped: 0,
        },
        false,
    );
    inventory
        .register_generated(&[bag.clone(), child.clone()], &[container(10, None)])
        .unwrap();
    let authority = InventoryAuthority {
        actor: EntityId(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: false,
        source_view: None,
        destination_view: None,
        new_item: None,
    };
    let op = inventory
        .apply(
            InventoryRequest::Move {
                item: EntityId(10),
                container: EntityId(1),
                placement: 0,
            },
            authority,
        )
        .unwrap();
    let ticket = inventory.take_proposal().unwrap();
    assert_eq!(
        inventory.generated_changes(op).unwrap(),
        vec![EntityId(10), EntityId(11)]
    );
    assert!(inventory.reserved(EntityId(11)));
    assert_eq!(inventory.item(EntityId(10)), Some(&bag));
    assert_eq!(inventory.item(EntityId(11)), Some(&child));
    let receipt = InventoryReceipt {
        operation: op,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    let mut wrong = receipt.clone();
    wrong.revisions.pop();
    assert!(inventory.confirm(&wrong).is_err());
    inventory.confirm(&receipt).unwrap();
    inventory.adopt_generated_durability(&[EntityId(10), EntityId(11)]);
    assert!(inventory.owned(EntityId(1), EntityId(11)));
    assert!(inventory.transient.is_empty());
    assert_eq!(inventory.item(EntityId(11)).unwrap().revision, 1);
}
#[test]
fn rejected_generated_batch_has_no_partial_items_or_containers() {
    let mut inventory = Inventory::new(20);
    let bad = item(
        10,
        ItemPlace::Contained {
            container: EntityId(99),
            slot: 0,
            equipped: 0,
        },
        false,
    );
    assert!(
        inventory
            .register_generated(&[bad], &[container(10, None)])
            .is_err()
    );
    assert!(inventory.items.is_empty());
    assert!(inventory.containers.is_empty());
    assert!(inventory.transient.is_empty());
}

#[test]
fn durable_bag_crossing_world_fences_every_child_without_inventing_mutations() {
    let mut inventory = Inventory::new(20);
    inventory.register_container(container(1, Some(1))).unwrap();
    inventory
        .register_container(container(10, Some(1)))
        .unwrap();
    inventory
        .register_container(container(11, Some(1)))
        .unwrap();
    let inside = |parent, slot| ItemPlace::Contained {
        container: EntityId(parent),
        slot,
        equipped: 0,
    };
    for value in [
        item(10, inside(1, 0), true),
        item(11, inside(10, 0), true),
        item(12, inside(11, 0), false),
    ] {
        inventory.register_item(value).unwrap();
    }
    let auth = InventoryAuthority {
        actor: EntityId(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: true,
        source_view: None,
        destination_view: None,
        new_item: None,
    };
    let operation = inventory
        .apply(InventoryRequest::Drop { item: EntityId(10) }, auth)
        .unwrap();
    let proposal = inventory.take_proposal().unwrap();
    assert_eq!(proposal.proposal.changes.len(), 3);
    for child in [EntityId(11), EntityId(12)] {
        let change = proposal
            .proposal
            .changes
            .iter()
            .find(|c| c.after.id == child)
            .unwrap();
        assert_eq!(change.before.as_ref(), Some(&change.after));
        assert!(inventory.reserved(child));
    }
    let receipt = InventoryReceipt {
        operation,
        revisions: proposal
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    let mut missing = receipt.clone();
    missing.revisions.pop();
    assert!(inventory.confirm(&missing).is_err());
    assert!(inventory.owned(EntityId(1), EntityId(12)));
    inventory.confirm(&receipt).unwrap();
    assert!(!inventory.owned(EntityId(1), EntityId(12)));
    assert_eq!(inventory.containers[&EntityId(10)].root_owner, None);
    assert_eq!(inventory.containers[&EntityId(11)].root_owner, None);
    assert_eq!(inventory.containers[&EntityId(10)].revision, 2);
    let operation = inventory
        .apply(
            InventoryRequest::Move {
                item: EntityId(10),
                container: EntityId(1),
                placement: 0,
            },
            auth,
        )
        .unwrap();
    let proposal = inventory.take_proposal().unwrap();
    assert_eq!(proposal.proposal.changes.len(), 3);
    inventory
        .confirm(&InventoryReceipt {
            operation,
            revisions: proposal
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        })
        .unwrap();
    assert!(inventory.owned(EntityId(1), EntityId(12)));
    assert_eq!(inventory.item(EntityId(12)).unwrap().revision, 1);
    assert_eq!(
        inventory.containers[&EntityId(11)].root_owner,
        Some(EntityId(1))
    );
}
