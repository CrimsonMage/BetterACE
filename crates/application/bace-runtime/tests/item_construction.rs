use bace_content::{Position, Property, WeenieV1};
use bace_inventory::{InventoryProposal, ItemChange, ItemPlace};
use bace_runtime::game_inventory::*;
use bace_storage_codec::*;
use bace_types::EntityId;
use std::collections::BTreeMap;
fn fixture() -> (
    Vec<FrozenInventoryItem>,
    InventoryProposal,
    BTreeMap<u32, Position>,
) {
    let mut root = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "creature".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    root.properties.ints.push(Property { id: 6, value: 4 });
    let mut child = root.clone();
    child.weenie_id = 101;
    child.class_name = "child".into();
    child.weenie_type = 1;
    let original = ItemPlace::Contained {
        container: EntityId(900),
        slot: 0,
        equipped: 0,
    };
    let (before, _) =
        bace_runtime::generator_items::prepare_inventory_item(&root, EntityId(500), 1, original)
            .unwrap();
    let (child_item, _) = bace_runtime::generator_items::prepare_inventory_item(
        &child,
        EntityId(501),
        1,
        ItemPlace::Contained {
            container: EntityId(500),
            slot: 0,
            equipped: 0,
        },
    )
    .unwrap();
    let construction = FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 900,
            incarnation: 1,
            content_revision: 1,
            profile: 0,
            occurrence: 0,
            random_identity: [2; 16],
            random_key_version: 1,
        },
        equipment_order: vec![],
        death_roster: vec![FrozenConstructedChildV1 {
            entity: 501,
            parent: None,
        }],
    };
    let items = vec![
        FrozenInventoryItem {
            source_destination: None,
            corpse: None,
            entity: EntitySaveV1 {
                object_id: 500,
                template_revision: 1,
                mutation_revision: 1,
                state: root,
            },
            placement: Some(ItemPlacementV2::Contained {
                container: 900,
                slot: 0,
                pack_slot: false,
                equipped: 0,
            }),
            persisted_version: 1,
            enchantments: vec![],
            construction: Some(construction),
        },
        FrozenInventoryItem {
            source_destination: None,
            corpse: None,
            entity: EntitySaveV1 {
                object_id: 501,
                template_revision: 1,
                mutation_revision: 1,
                state: child,
            },
            placement: Some(ItemPlacementV2::Contained {
                container: 500,
                slot: 0,
                pack_slot: false,
                equipped: 0,
            }),
            persisted_version: 1,
            enchantments: vec![],
            construction: None,
        },
    ];
    let mut after = before.clone();
    after.place = ItemPlace::World;
    after.revision = 2;
    let proposal = InventoryProposal {
        changes: vec![
            ItemChange {
                before: Some(before),
                after,
            },
            ItemChange {
                before: Some(child_item.clone()),
                after: child_item,
            },
        ],
        participants: vec![(EntityId(500), 1), (EntityId(501), 1), (EntityId(900), 1)],
        actor_burden: 0,
        requires_pickup_motion: true,
    };
    let positions = BTreeMap::from([(
        500,
        Position {
            obj_cell_id: 0x12340001,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        },
    )]);
    (items, proposal, positions)
}
fn freeze(
    items: &[FrozenInventoryItem],
    proposal: &InventoryProposal,
    positions: &BTreeMap<u32, Position>,
) -> Result<bace_persistence::PlacementOperation, InventoryFreezeError> {
    freeze_inventory(InventoryFreezeInput {
        operation_id: "constructed-transfer",
        proposal,
        items,
        other_snapshots: &[],
        leases: &[],
        storage_views: &[],
        admitted_positions: positions,
    })
}
#[test]
fn transfer_retains_subtype_origin_roster_and_unchanged_child_revision() {
    let (items, proposal, positions) = fixture();
    let op = freeze(&items, &proposal, &positions).unwrap();
    let root = bace_storage_codec::ItemSaveV5::decode_or_migrate(
        &op.snapshots
            .iter()
            .find(|s| s.object_id == 500)
            .unwrap()
            .bytes,
        None,
    )
    .unwrap();
    let child = bace_storage_codec::ItemSaveV5::decode_or_migrate(
        &op.snapshots
            .iter()
            .find(|s| s.object_id == 501)
            .unwrap()
            .bytes,
        None,
    )
    .unwrap();
    assert_eq!(root.construction, items[0].construction);
    assert_eq!(root.entity.mutation_revision, 2);
    assert_eq!(child.entity.mutation_revision, 1);
    assert_eq!(child.placement, items[1].placement.clone().unwrap());
    assert_eq!(
        root.placement,
        ItemPlacementV2::World(positions[&500].clone())
    );
}
#[test]
fn missing_companion_missing_child_and_unjournaled_roster_change_fail_closed() {
    let (mut items, mut proposal, positions) = fixture();
    let saved = items[0].construction.take();
    assert!(freeze(&items, &proposal, &positions).is_err());
    items[0].construction = saved;
    assert!(freeze(&items[..1], &proposal, &positions).is_err());
    proposal.changes[1].after.place = ItemPlace::World;
    assert!(freeze(&items, &proposal, &positions).is_err());
    proposal.changes[1].after.place = ItemPlace::Contained {
        container: EntityId(500),
        slot: 0,
        equipped: 1,
    };
    assert!(freeze(&items, &proposal, &positions).is_err());
}
