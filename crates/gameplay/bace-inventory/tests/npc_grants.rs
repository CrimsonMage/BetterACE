use bace_inventory::{
    InventoryContainer, InventoryItem, InventoryView, ItemPlace, propose_npc_grants,
};
use bace_types::EntityId;
fn item(id: u32, container: u32, slot: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 100,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(container),
            slot,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 10,
        unit_burden: 2,
        unit_value: 3,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: false,
    }
}
fn container(id: u32, slots: u32) -> InventoryContainer {
    InventoryContainer {
        id: EntityId(id),
        revision: 3,
        root_owner: Some(EntityId(1)),
        slots,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    }
}
#[test]
fn batches_follow_source_main_zero_then_direct_side_pack_order() {
    let mut bag = item(20, 1, 0);
    bag.pack_slot = true;
    bag.is_container = true;
    bag.template = 200;
    let items = [item(10, 1, 0), bag];
    let containers = [container(1, 2), container(20, 2)];
    let fresh = [
        item(0x80000010, 1, 99),
        item(0x80000011, 1, 99),
        item(0x80000012, 1, 99),
    ];
    let proposal = propose_npc_grants(
        EntityId(1),
        &fresh,
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    let place = |id| {
        proposal
            .changes
            .iter()
            .find(|c| c.after.id == EntityId(id))
            .unwrap()
            .after
            .place
    };
    assert_eq!(
        place(10),
        ItemPlace::Contained {
            container: EntityId(1),
            slot: 1,
            equipped: 0
        }
    );
    assert_eq!(
        place(0x80000010),
        ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0
        }
    );
    assert_eq!(
        place(0x80000011),
        ItemPlace::Contained {
            container: EntityId(20),
            slot: 1,
            equipped: 0
        }
    );
    assert_eq!(
        place(0x80000012),
        ItemPlace::Contained {
            container: EntityId(20),
            slot: 0,
            equipped: 0
        }
    );
    assert_eq!(proposal.actor_burden, 10);
    assert_eq!(
        items[0].place,
        ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0
        }
    );
    assert_eq!(
        proposal
            .changes
            .iter()
            .find(|c| c.after.id == EntityId(10))
            .unwrap()
            .after
            .revision,
        2
    );
    assert!(proposal.participants.contains(&(EntityId(20), 1)));
    assert!(proposal.participants.contains(&(EntityId(0x80000012), 0)));
}
#[test]
fn capacity_unique_and_revision_failures_leave_whole_batch_unmodified() {
    let mut old = item(10, 1, 0);
    old.revision = u64::MAX;
    let mut root = container(1, 1);
    let fresh = [item(0x80000010, 1, 0), item(0x80000011, 1, 0)];
    assert!(
        propose_npc_grants(
            EntityId(1),
            &fresh,
            InventoryView {
                items: &[old.clone()],
                containers: &[root]
            }
        )
        .is_err()
    );
    root.slots = 4;
    assert!(
        propose_npc_grants(
            EntityId(1),
            &fresh,
            InventoryView {
                items: &[old.clone()],
                containers: &[root]
            }
        )
        .is_err()
    );
    old.revision = 1;
    let mut unique = fresh[0].clone();
    unique.unique = true;
    assert!(
        propose_npc_grants(
            EntityId(1),
            &[unique],
            InventoryView {
                items: &[old],
                containers: &[root]
            }
        )
        .is_err()
    );
    let duplicate = [fresh[0].clone(), fresh[0].clone()];
    assert!(
        propose_npc_grants(
            EntityId(1),
            &duplicate,
            InventoryView {
                items: &[],
                containers: &[root]
            }
        )
        .is_err()
    );
}
