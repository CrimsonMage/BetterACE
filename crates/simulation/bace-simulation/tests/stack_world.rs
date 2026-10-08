#[allow(dead_code, unused_imports)]
mod magic_common;
use bace_inventory::{
    InventoryAuthority, InventoryContainer, InventoryItem, ItemPlace, StackSplitPreparation,
};
use bace_simulation::{InventoryReceipt, PreparedStackDrop};
use magic_common::*;
const SOURCE: EntityId = EntityId(0x80000010);
const FRESH: EntityId = EntityId(0x80000011);
fn item(id: EntityId) -> InventoryItem {
    InventoryItem {
        id,
        revision: 1,
        template: 500,
        structure: None,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 10,
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
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
fn preparation() -> StackSplitPreparation {
    let mut fresh = item(FRESH);
    fresh.revision = 0;
    fresh.stack = 3;
    fresh.place = ItemPlace::World;
    StackSplitPreparation {
        fresh,
        source_stackable: true,
        source_stuck: false,
        source_vendor: false,
        destination_corpse: false,
    }
}
fn authority() -> InventoryAuthority {
    InventoryAuthority {
        actor: EntityId(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: true,
        source_view: None,
        destination_view: None,
        new_item: Some(FRESH),
    }
}
fn drop(k: &Kernel, cell: u32) -> PreparedStackDrop {
    PreparedStackDrop {
        source_epoch: k.world().body(EntityId(1)).unwrap().accepted().epoch(),
        spawn: bace_physics::GeometrySpawn {
            cell,
            position: Vec3::new(15., 15., 0.2),
            shape: Arc::new(
                bace_physics::CollisionShape::prepare(
                    vec![bace_physics::CollisionSphere {
                        center: Vec3::new(0., 0., 0.2),
                        radius: 0.2,
                    }],
                    0.,
                    0.1,
                )
                .unwrap(),
            ),
            capabilities: Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
            heading: 0.,
            maximum_turn_rate: 1.,
        },
    }
}
fn fixture() -> Kernel {
    let mut k = kernel_fixture(64, false, true);
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 1,
        burden_limit: 1000,
        accessible: true,
        open: true,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(item(SOURCE)).unwrap();
    k
}
fn request() -> bace_gameplay_api::InventoryRequest {
    bace_gameplay_api::InventoryRequest::SplitToWorld {
        item: SOURCE,
        amount: 3,
    }
}
#[test]
fn split_world_requires_geometry_and_exact_receipt_without_early_mutation() {
    let mut k = fixture();
    assert!(
        k.propose_stack_split(context(1), request(), authority(), preparation())
            .is_err()
    );
    let missing = drop(&k, 2);
    assert!(
        k.propose_stack_split_to_world(context(1), request(), authority(), preparation(), missing)
            .is_err()
    );
    assert_eq!(k.inventory_item(SOURCE).unwrap().stack, 10);
    assert!(k.inventory_item(FRESH).is_none());
    let physical = drop(&k, 1);
    let op = k
        .propose_stack_split_to_world(context(1), request(), authority(), preparation(), physical)
        .unwrap();
    let placement = k.inventory_world_placement(op).unwrap();
    assert_eq!(placement.item, FRESH);
    assert!(!k.world().contains_identity(FRESH));
    let ticket = k.take_inventory_proposal().unwrap();
    assert!(
        k.confirm_inventory_committed(&InventoryReceipt {
            operation: op,
            revisions: vec![]
        })
        .is_err()
    );
    assert_eq!(k.inventory_item(SOURCE).unwrap().stack, 10);
    let receipt = InventoryReceipt {
        operation: op,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    k.confirm_inventory_committed(&receipt).unwrap();
    assert_eq!(k.inventory_item(SOURCE).unwrap().stack, 7);
    assert_eq!(k.inventory_item(FRESH).unwrap().stack, 3);
    assert_eq!(k.inventory_item(FRESH).unwrap().place, ItemPlace::World);
    assert!(k.world().contains_identity(FRESH));
    assert_eq!(k.take_stack_world_placement(), Some(placement));
    assert!(k.confirm_inventory_committed(&receipt).is_err());
    assert!(k.take_stack_world_placement().is_none());
}
#[test]
fn rejected_world_split_preserves_source_and_releases_fresh_identity() {
    let mut k = fixture();
    let physical = drop(&k, 1);
    let op = k
        .propose_stack_split_to_world(context(1), request(), authority(), preparation(), physical)
        .unwrap();
    k.reject_inventory(op).unwrap();
    assert_eq!(k.inventory_item(SOURCE).unwrap().stack, 10);
    assert!(k.inventory_item(FRESH).is_none());
    assert!(!k.world().contains_identity(FRESH));
    assert!(k.inventory_world_placement(op).is_none());
    assert!(k.take_stack_world_placement().is_none());
    let physical = drop(&k, 1);
    k.propose_stack_split_to_world(context(2), request(), authority(), preparation(), physical)
        .unwrap();
}
