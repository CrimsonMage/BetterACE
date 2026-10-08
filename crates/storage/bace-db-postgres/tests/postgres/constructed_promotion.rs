use super::Cluster;
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{
    ConstructedCreaturePromotionOperation, DurableItemPlace, OperationOutcome, PlacementChange,
    PlacementOperation, SaveSnapshot, WorldPlacementOperation,
};
use bace_storage_codec::{
    EntitySaveV1, FrozenConstructedChildV1, FrozenCreatureConstructionV1,
    FrozenGeneratorConstructionOriginV1, ItemPlacementV2, ItemSaveV2, ItemSaveV3, ItemSaveV4,
    ItemSaveV5,
};

fn pose() -> bace_content::Position {
    bace_content::Position {
        obj_cell_id: 0x1234_0100,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    }
}

fn item(
    id: u32,
    weenie_type: u32,
    placement: ItemPlacementV2,
    construction: Option<FrozenCreatureConstructionV1>,
) -> Vec<u8> {
    let entity = EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "promotion_fixture".into(),
            weenie_type,
            last_modified: None,
            properties: Default::default(),
        },
    };
    let v2 = ItemSaveV2::migrate_v1(entity, placement).unwrap();
    let v3 = ItemSaveV3::migrate_v2(v2).unwrap();
    ItemSaveV5 {
        previous: ItemSaveV4 {
            previous: v3,
            construction,
        },
        source_destination: Some(2),
    }
    .encode()
    .unwrap()
}

fn snapshot(id: u32, bytes: Vec<u8>) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        mutation_revision: 1,
        expected_version: 0,
        bytes,
    }
}

fn contained(container: u32, equipped: u32) -> ItemPlacementV2 {
    ItemPlacementV2::Contained {
        container,
        slot: 0,
        pack_slot: false,
        equipped,
    }
}

fn durable_contained(container: u32, equipped: u32) -> DurableItemPlace {
    DurableItemPlace::Contained {
        container,
        slot: 0,
        pack_slot: false,
        equipped,
    }
}

#[tokio::test]
async fn constructed_promotion_requires_complete_v5_graph_before_exact_receipt() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let parent = 0x8000_2600;
    let creature = 0x8000_2601;
    let gear = 0x8000_2602;
    let bag = 0x8000_2603;
    let create_parent = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "constructed-parent".into(),
            snapshots: vec![snapshot(
                parent,
                item(parent, 1, ItemPlacementV2::World(pose()), None),
            )],
            participants: vec![parent],
            leases: vec![],
            changes: vec![PlacementChange {
                item: parent,
                expected: None,
                destination: DurableItemPlace::World {
                    cell: pose().obj_cell_id,
                },
            }],
            storage_views: vec![],
        },
    };
    store
        .world_placement_operation(&create_parent)
        .await
        .unwrap();
    let companion = FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: parent,
            incarnation: 2,
            content_revision: 3,
            profile: 4,
            occurrence: 5,
            random_identity: [9; 16],
            random_key_version: 1,
        },
        equipment_order: vec![gear],
        death_roster: vec![
            FrozenConstructedChildV1 {
                entity: bag,
                parent: None,
            },
            FrozenConstructedChildV1 {
                entity: gear,
                parent: None,
            },
        ],
    };
    let operation = ConstructedCreaturePromotionOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "constructed-promote-1".into(),
            snapshots: vec![
                snapshot(
                    creature,
                    item(creature, 10, contained(parent, 0), Some(companion.clone())),
                ),
                snapshot(
                    bag,
                    item(
                        bag,
                        1,
                        ItemPlacementV2::Contained {
                            container: creature,
                            slot: 1,
                            pack_slot: false,
                            equipped: 0,
                        },
                        None,
                    ),
                ),
                snapshot(gear, item(gear, 1, contained(creature, 1), None)),
            ],
            participants: vec![parent, creature, gear, bag],
            leases: vec![],
            changes: vec![
                PlacementChange {
                    item: creature,
                    expected: None,
                    destination: durable_contained(parent, 0),
                },
                PlacementChange {
                    item: bag,
                    expected: None,
                    destination: DurableItemPlace::Contained {
                        container: creature,
                        slot: 1,
                        pack_slot: false,
                        equipped: 0,
                    },
                },
                PlacementChange {
                    item: gear,
                    expected: None,
                    destination: durable_contained(creature, 1),
                },
            ],
            storage_views: vec![],
        },
        creature_roots: vec![creature],
    };
    let generic = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: operation.inventory.clone(),
    };
    assert!(matches!(
        store.world_placement_operation(&generic).await,
        Err(StoreError::Invalid(_))
    ));
    let mut broken = operation.clone();
    let mut wrong = companion.clone();
    wrong.equipment_order.clear();
    broken.inventory.snapshots[0].bytes = item(creature, 10, contained(parent, 0), Some(wrong));
    assert!(matches!(
        store.constructed_creature_promotion(&broken).await,
        Err(StoreError::Invalid(_))
    ));
    let mut wrong_parent = operation.clone();
    let mut wrong = companion.clone();
    wrong.death_roster[1].parent = Some(bag);
    wrong_parent.inventory.snapshots[0].bytes =
        item(creature, 10, contained(parent, 0), Some(wrong));
    assert!(matches!(
        store.constructed_creature_promotion(&wrong_parent).await,
        Err(StoreError::Invalid(_))
    ));
    assert!(store.load(creature).await.unwrap().is_none());
    assert!(store.load(gear).await.unwrap().is_none());
    assert!(store.load(bag).await.unwrap().is_none());
    assert!(
        store
            .resolve_operation(&operation.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );

    let OperationOutcome::Committed(acks) = store
        .constructed_creature_promotion(&operation)
        .await
        .unwrap()
    else {
        panic!("promotion must commit");
    };
    assert_eq!(acks.len(), 3);
    assert!(
        acks.iter()
            .any(|ack| ack.object_id == creature && ack.persisted_version == 1)
    );
    assert!(
        acks.iter()
            .any(|ack| ack.object_id == gear && ack.persisted_version == 1)
    );
    assert_eq!(
        store
            .constructed_creature_promotion(&operation)
            .await
            .unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut mismatched = operation.clone();
    let mut changed_gear = ItemSaveV5::decode(&mismatched.inventory.snapshots[2].bytes).unwrap();
    changed_gear.source_destination = Some(4);
    mismatched.inventory.snapshots[2].bytes = changed_gear.encode().unwrap();
    assert!(matches!(
        store.constructed_creature_promotion(&mismatched).await,
        Err(StoreError::OperationMismatch)
    ));
    let mut changed = operation.clone();
    changed.creature_roots.reverse();
    changed.creature_roots.push(gear);
    assert!(matches!(
        store.constructed_creature_promotion(&changed).await,
        Err(StoreError::Invalid(_))
    ));
    let tree = store
        .load_world_item_tree(
            pose().obj_cell_id,
            bace_persistence::InventoryLoadLimits::default(),
        )
        .await
        .unwrap();
    assert_eq!(tree.len(), 4);
    let stored = ItemSaveV5::decode(&store.load(creature).await.unwrap().unwrap().bytes).unwrap();
    assert_eq!(stored.construction.as_ref().unwrap(), &companion);
    owner.close().await.unwrap();
    store.close().await;
}

#[tokio::test]
async fn nested_creature_promotion_partitions_each_equipment_owner() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let parent = 0x8000_2700;
    let outer = 0x8000_2701;
    let inner = 0x8000_2702;
    let gear = 0x8000_2703;
    store
        .world_placement_operation(&WorldPlacementOperation {
            world_epoch: owner.epoch(),
            inventory: PlacementOperation {
                operation_id: "nested-construction-parent".into(),
                snapshots: vec![snapshot(
                    parent,
                    item(parent, 1, ItemPlacementV2::World(pose()), None),
                )],
                participants: vec![parent],
                leases: vec![],
                changes: vec![PlacementChange {
                    item: parent,
                    expected: None,
                    destination: DurableItemPlace::World {
                        cell: pose().obj_cell_id,
                    },
                }],
                storage_views: vec![],
            },
        })
        .await
        .unwrap();
    let origin = FrozenGeneratorConstructionOriginV1 {
        generator: parent,
        incarnation: 1,
        content_revision: 1,
        profile: 1,
        occurrence: 1,
        random_identity: [4; 16],
        random_key_version: 1,
    };
    let outer_companion = FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: origin.clone(),
        equipment_order: vec![],
        death_roster: vec![],
    };
    let inner_companion = FrozenCreatureConstructionV1 {
        weenie_type: 15,
        origin,
        equipment_order: vec![gear],
        death_roster: vec![FrozenConstructedChildV1 {
            entity: gear,
            parent: None,
        }],
    };
    let operation = ConstructedCreaturePromotionOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "nested-constructed-promote".into(),
            snapshots: vec![
                snapshot(
                    outer,
                    item(
                        outer,
                        10,
                        contained(parent, 0),
                        Some(outer_companion.clone()),
                    ),
                ),
                snapshot(
                    inner,
                    item(
                        inner,
                        15,
                        contained(outer, 0),
                        Some(inner_companion.clone()),
                    ),
                ),
                snapshot(gear, item(gear, 1, contained(inner, 1), None)),
            ],
            participants: vec![parent, outer, inner, gear],
            leases: vec![],
            changes: vec![
                PlacementChange {
                    item: outer,
                    expected: None,
                    destination: durable_contained(parent, 0),
                },
                PlacementChange {
                    item: inner,
                    expected: None,
                    destination: durable_contained(outer, 0),
                },
                PlacementChange {
                    item: gear,
                    expected: None,
                    destination: durable_contained(inner, 1),
                },
            ],
            storage_views: vec![],
        },
        creature_roots: vec![outer, inner],
    };
    let mut wrong_owner = operation.clone();
    let mut wrong = outer_companion;
    wrong.equipment_order.push(gear);
    wrong_owner.inventory.snapshots[0].bytes = item(outer, 10, contained(parent, 0), Some(wrong));
    assert!(matches!(
        store.constructed_creature_promotion(&wrong_owner).await,
        Err(StoreError::Invalid(_))
    ));
    assert!(store.load(outer).await.unwrap().is_none());
    assert!(matches!(
        store
            .constructed_creature_promotion(&operation)
            .await
            .unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        ItemSaveV5::decode(&store.load(inner).await.unwrap().unwrap().bytes)
            .unwrap()
            .construction
            .as_ref(),
        Some(&inner_companion)
    );
    owner.close().await.unwrap();
    store.close().await;
}
