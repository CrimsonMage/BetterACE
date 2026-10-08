use super::*;
use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_physics::{Body, SyntheticScene};
fn item(id: u32, place: ItemPlace, container: bool) -> InventoryItem {
    InventoryItem {
        id: EntityId(id),
        revision: 7,
        template: 10,
        stack_key: 1,
        place,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: container,
        is_container: container,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
        structure: None,
    }
}
fn fixture() -> Kernel {
    let mut world = World::default();
    world
        .register_scene(
            CellId(1),
            SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let body = Body::spawn(
        world.scene(CellId(1)).unwrap(),
        Vec3::new(1., 2., 0.5),
        0.5,
        bace_motion::Capabilities {
            speed: 1.,
            jump_impulse: 1.,
        },
    )
    .unwrap();
    world
        .insert(Actor {
            id: EntityId(10),
            cell: CellId(1),
            body,
        })
        .unwrap();
    let mut k = Kernel::with_gameplay_limits(world, 8, 8, 64).unwrap();
    k.register_inventory_container(InventoryContainer {
        id: EntityId(10),
        revision: 7,
        root_owner: None,
        slots: 10,
        pack_slots: 10,
        burden_limit: 100,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(item(10, ItemPlace::World, true))
        .unwrap();
    k.register_inventory_item(item(
        11,
        ItemPlace::Contained {
            container: EntityId(10),
            slot: 0,
            equipped: 0,
        },
        false,
    ))
    .unwrap();
    k.register_magic_registry(
        EntityId(10),
        bace_magic::EnchantmentRegistry::restore(8, 3, vec![]).unwrap(),
        false,
    )
    .unwrap();
    let epoch = k.request_region(0, false).unwrap();
    k.acknowledge_region_admission(0, epoch, false).unwrap();
    k.region_residency.advance(10000).unwrap();
    k.region_residency.advance(10150).unwrap();
    assert_eq!(
        k.region_residency.state(0).unwrap().phase,
        crate::RegionPhase::Draining
    );
    k
}
#[test]
fn durable_tree_and_registry_are_held_until_every_exact_save_receipt() {
    let mut k = fixture();
    k.register_native_npc(
        EntityId(10),
        std::sync::Arc::new(
            bace_emotes::NativeProgram::prepare(vec![], Default::default()).unwrap(),
        ),
        3.0,
    )
    .unwrap();
    k.step_region_unloads();
    let ticket = k.take_region_unload_proposal().unwrap();
    assert_eq!(
        k.start_npc_emote(EntityId(10), None, Default::default(), [1; 16], 1, false),
        Err(bace_emotes::NativeError::Busy)
    );
    assert_eq!(ticket.items.len(), 2);
    assert_eq!(ticket.items[0].position.unwrap().origin, [1., 2., 0.5]);
    assert_eq!(ticket.items[0].registry_revision, Some(3));
    assert!(k.inventory.reserved(EntityId(10)) && k.inventory.reserved(EntityId(11)));
    assert!(
        k.register_inventory_item(item(
            12,
            ItemPlace::Contained {
                container: EntityId(10),
                slot: 1,
                equipped: 0
            },
            false
        ))
        .is_err()
    );
    k.retry_region_unload(ticket.operation).unwrap();
    assert_eq!(k.take_region_unload_proposal().unwrap(), ticket);
    let mut receipt = RegionUnloadReceipt {
        operation: ticket.operation,
        landblock: 0,
        epoch: ticket.epoch,
        revisions: ticket
            .items
            .iter()
            .map(|i| (i.item.id, i.item.revision, i.registry_revision))
            .collect(),
    };
    receipt.revisions[0].2 = Some(4);
    assert_eq!(k.confirm_region_unload_saved(&receipt), Err(E::Stale));
    k.step_region_unloads();
    assert!(k.world.contains_identity(EntityId(10)));
    assert!(k.npcs.has_source(EntityId(10)));
    receipt.revisions[0].2 = Some(3);
    k.confirm_region_unload_saved(&receipt).unwrap();
    k.step_region_unloads();
    assert!(k.region_residency.state(0).is_none());
    assert!(k.inventory.item(EntityId(10)).is_none());
    assert!(k.inventory.item(EntityId(11)).is_none());
    assert!(k.inventory.container(EntityId(10)).is_none());
    assert!(k.magic.registry(EntityId(10)).is_none());
    assert!(k.world.scene(CellId(1)).is_err());
    assert!(!k.npcs.has_source(EntityId(10)));
}
#[test]
fn stale_epoch_save_never_releases_region() {
    let mut k = fixture();
    k.step_region_unloads();
    let t = k.take_region_unload_proposal().unwrap();
    let receipt = RegionUnloadReceipt {
        operation: t.operation,
        landblock: t.landblock,
        epoch: t.epoch + 1,
        revisions: t
            .items
            .iter()
            .map(|i| (i.item.id, i.item.revision, i.registry_revision))
            .collect(),
    };
    assert_eq!(k.confirm_region_unload_saved(&receipt), Err(E::Stale));
    assert!(k.inventory.reserved(EntityId(11)));
}

#[test]
fn registry_dirty_touch_keeps_item_and_container_revision_in_sync() {
    let mut k = fixture();
    assert!(k.inventory.touch_registry(EntityId(10)).unwrap());
    assert_eq!(k.inventory.item(EntityId(10)).unwrap().revision, 8);
    assert_eq!(k.inventory.container(EntityId(10)).unwrap().revision, 8);
    k.step_region_unloads();
    let t = k.take_region_unload_proposal().unwrap();
    assert_eq!(
        t.items[0].item.revision,
        t.items[0].container.unwrap().revision
    );
    assert!(!k.inventory.touch_registry(EntityId(10)).unwrap());
    assert_eq!(k.inventory.item(EntityId(10)).unwrap().revision, 8);
}
