use super::*;
fn container(id: u32, owner: u32) -> InventoryContainer {
    InventoryContainer {
        id: EntityId(id),
        revision: 1,
        root_owner: Some(EntityId(owner)),
        slots: 16,
        pack_slots: 8,
        burden_limit: 10000,
        accessible: true,
        open: true,
        generation: 1,
    }
}
fn item(id: u32, parent: u32, count: u32, burden: u32, bag: bool) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: id,
        stack_key: u64::from(id),
        place: ItemPlace::Contained {
            container: EntityId(parent),
            slot: 0,
            equipped: 0,
        },
        stack: count,
        maximum_stack: if bag { 1 } else { 100 },
        unit_burden: burden,
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
fn commit(inventory: &mut Inventory, operation: u64) {
    let receipt = InventoryReceipt {
        operation,
        revisions: inventory
            .pending_ticket(operation)
            .unwrap()
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    inventory.claim(operation).unwrap();
    inventory.confirm(&receipt).unwrap();
}
#[test]
fn nested_accepted_burden_changes_only_on_commit_and_keeps_unrelated_cache() {
    let mut inventory = Inventory::new(32);
    for (id, owner) in [(1, 1), (2, 2), (101, 1)] {
        inventory.register_container(container(id, owner)).unwrap();
    }
    inventory.register_item(item(101, 1, 1, 5, true)).unwrap();
    inventory
        .register_item(item(102, 101, 10, 2, false))
        .unwrap();
    assert_eq!(inventory.actor_burden(EntityId(1)), Ok(25));
    assert_eq!(inventory.actor_burden(EntityId(2)), Ok(0));
    let operation = inventory.take(EntityId(1), EntityId(102), 3).unwrap();
    assert_eq!(inventory.actor_burden(EntityId(1)), Ok(25));
    commit(&mut inventory, operation);
    assert_eq!(inventory.burden.get(&EntityId(2)), Some(&0));
    assert_eq!(inventory.actor_burden(EntityId(1)), Ok(19));
    let rejected = inventory.take(EntityId(1), EntityId(102), 2).unwrap();
    inventory.reject(rejected).unwrap();
    assert_eq!(inventory.actor_burden(EntityId(1)), Ok(19));
    // Move the root bag with its unchanged contents to another accepted owner.
    let before = inventory.item(EntityId(101)).unwrap().clone();
    let mut after = before.clone();
    after.place = ItemPlace::Contained {
        container: EntityId(2),
        slot: 0,
        equipped: 0,
    };
    after.revision += 1;
    let items: Vec<_> = inventory.items.values().cloned().collect();
    let containers: Vec<_> = inventory.containers.values().copied().collect();
    let proposal = bace_inventory::propose_item_changes(
        EntityId(1),
        vec![bace_inventory::ItemChange {
            before: Some(before),
            after,
        }],
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    let operation = inventory.reserve(EntityId(1), proposal).unwrap();
    commit(&mut inventory, operation);
    assert_eq!(inventory.actor_burden(EntityId(1)), Ok(0));
    assert_eq!(inventory.actor_burden(EntityId(2)), Ok(19));
}
#[test]
fn malformed_parent_cycles_are_errors_and_are_not_cached_as_zero_weight() {
    let mut inventory = Inventory::new(32);
    inventory.register_container(container(1, 1)).unwrap();
    inventory.register_container(container(10, 1)).unwrap();
    inventory.register_container(container(11, 1)).unwrap();
    inventory.register_item(item(10, 11, 1, 1, true)).unwrap();
    inventory.register_item(item(11, 10, 1, 1, true)).unwrap();
    assert_eq!(
        inventory.actor_burden(EntityId(1)),
        Err(Error::InvalidState)
    );
    assert!(!inventory.burden.contains_key(&EntityId(1)));
}
