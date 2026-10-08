use super::*;
use bace_inventory::{InventoryContainer, InventoryItem, InventoryProposal, ItemChange};
const CORPSE: EntityId = EntityId(40);
fn item(id: u32, place: ItemPlace, is_container: bool) -> InventoryItem {
    InventoryItem {
        id: EntityId(id),
        revision: 1,
        template: 100 + id,
        structure: None,
        stack_key: u64::from(id),
        place,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 0,
        unit_value: 0,
        pack_slot: is_container,
        is_container,
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
fn fixture(two: bool) -> Kernel {
    let mut k = crate::kernel::magic_components_fixture::component_kernel(64);
    for (id, owner) in [(EntityId(1), Some(EntityId(1))), (CORPSE, None)] {
        k.inventory
            .register_container(InventoryContainer {
                id,
                revision: 1,
                root_owner: owner,
                slots: 10,
                pack_slots: 2,
                burden_limit: 1000,
                accessible: true,
                open: true,
                generation: 1,
            })
            .unwrap();
    }
    k.inventory
        .register_item(item(40, ItemPlace::World, true))
        .unwrap();
    for id in 41..=if two { 42 } else { 41 } {
        k.inventory
            .register_item(item(
                id,
                ItemPlace::Contained {
                    container: CORPSE,
                    slot: id - 41,
                    equipped: 0,
                },
                false,
            ))
            .unwrap();
    }
    k.corpse_expiry.deadlines.insert(
        CORPSE,
        crate::corpse_expiry::CorpseDeadline {
            operation: 77,
            tick: 10000,
        },
    );
    k
}
fn reserve(k: &mut Kernel) -> u64 {
    let before = k.inventory.item(EntityId(41)).unwrap().clone();
    let mut after = before.clone();
    after.place = ItemPlace::Contained {
        container: EntityId(1),
        slot: 0,
        equipped: 0,
    };
    after.revision += 1;
    k.inventory
        .reserve(
            EntityId(1),
            InventoryProposal {
                changes: vec![ItemChange {
                    before: Some(before),
                    after,
                }],
                participants: vec![(EntityId(1), 1), (CORPSE, 1), (EntityId(41), 1)],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        )
        .unwrap()
}
#[test]
fn final_loot_deadline_and_root_revision_wait_for_same_exact_receipt() {
    let mut k = fixture(false);
    k.tick = 100;
    let op = reserve(&mut k);
    let changes = k.prepare_inventory_corpse_decay(op).unwrap();
    assert_eq!(
        changes,
        vec![crate::CorpseDecayChange {
            corpse: CORPSE,
            death_operation: 77,
            before_expires_tick: 10000,
            after_expires_tick: 550,
            prepared_tick: 100,
        }]
    );
    assert_eq!(k.inventory.item(CORPSE).unwrap().revision, 1);
    assert_eq!(k.corpse_expiry.deadlines[&CORPSE].tick, 10000);
    k.inventory.claim(op).unwrap();
    let mut receipt = crate::InventoryReceipt {
        operation: op,
        revisions: vec![(EntityId(41), 2)],
    };
    assert!(k.inventory.confirm(&receipt).is_err());
    assert_eq!(k.corpse_expiry.deadlines[&CORPSE].tick, 10000);
    receipt.revisions.push((CORPSE, 2));
    k.validate_inventory_corpse_decay(&changes).unwrap();
    k.inventory.confirm(&receipt).unwrap();
    k.adopt_inventory_corpse_decay(&changes);
    assert_eq!(k.inventory.item(CORPSE).unwrap().revision, 2);
    assert_eq!(k.corpse_expiry.deadlines[&CORPSE].tick, 550);
}
#[test]
fn rejection_nonempty_and_shorter_deadline_preserve_original_expiry() {
    let mut k = fixture(false);
    let op = reserve(&mut k);
    assert_eq!(k.prepare_inventory_corpse_decay(op).unwrap().len(), 1);
    k.inventory.reject(op).unwrap();
    assert_eq!(k.inventory.item(CORPSE).unwrap().revision, 1);
    assert_eq!(k.corpse_expiry.deadlines[&CORPSE].tick, 10000);
    k.corpse_expiry.deadlines.get_mut(&CORPSE).unwrap().tick = 400;
    let op = reserve(&mut k);
    assert!(k.prepare_inventory_corpse_decay(op).unwrap().is_empty());
    assert_eq!(
        k.inventory
            .pending_ticket(op)
            .unwrap()
            .proposal
            .changes
            .len(),
        1
    );
    let mut k = fixture(true);
    let op = reserve(&mut k);
    assert!(k.prepare_inventory_corpse_decay(op).unwrap().is_empty());
    assert_eq!(k.corpse_expiry.deadlines[&CORPSE].tick, 10000);
}

#[test]
fn original_ace_empty_corpse_branch_vectors() {
    let trace = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/corpse_empty_decay.trace"
    ))
    .unwrap();
    let mut count = 0;
    for line in trace.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<u64> = line.split('|').map(|v| v.parse().unwrap()).collect();
        let mut k = fixture(row[0] != 0);
        k.corpse_expiry.deadlines.get_mut(&CORPSE).unwrap().tick = row[1];
        let op = reserve(&mut k);
        let change = k.prepare_inventory_corpse_decay(op).unwrap();
        assert_eq!(
            change.first().map_or(row[1], |c| c.after_expires_tick),
            row[2],
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 12);
}
