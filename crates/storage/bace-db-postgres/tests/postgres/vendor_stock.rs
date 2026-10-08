use super::Cluster;
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{
    DurableItemPlace, OperationOutcome, PlacementChange, PlacementOperation, SaveSnapshot,
    VendorStockOperation, VendorStockWrite, WorldPlacementOperation,
};
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV3, ItemSaveV4, ItemSaveV5,
    VendorDefaultStockV1, VendorStockSaveV1,
};

fn position() -> bace_content::Position {
    bace_content::Position {
        obj_cell_id: 0x1234_0100,
        position_x: 1.0,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    }
}

fn item(id: u32, placement: ItemPlacementV2) -> Vec<u8> {
    let entity = EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "vendor_stock_fixture".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    };
    let v2 = ItemSaveV2::migrate_v1(entity, placement).unwrap();
    let v3 = ItemSaveV3::migrate_v2(v2).unwrap();
    let v4 = ItemSaveV4::migrate_v3(v3).unwrap();
    ItemSaveV5 {
        previous: v4,
        source_destination: Some(4),
    }
    .encode()
    .unwrap()
}

fn snapshot(id: u32, bytes: Vec<u8>) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        expected_version: 0,
        mutation_revision: 1,
        bytes,
    }
}

#[tokio::test]
async fn stock_marker_and_shop_items_commit_or_rollback_together() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let vendor = 0x8000_1400;
    let marker_id = 0x8000_1401;
    let stock_id = 0x8000_1402;
    let vendor_create = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "vendor-durable-root".into(),
            snapshots: vec![snapshot(
                vendor,
                item(vendor, ItemPlacementV2::World(position())),
            )],
            participants: vec![vendor],
            leases: vec![],
            changes: vec![PlacementChange {
                item: vendor,
                expected: None,
                destination: DurableItemPlace::World {
                    cell: position().obj_cell_id,
                },
            }],
            storage_views: vec![],
        },
    };
    store
        .world_placement_operation(&vendor_create)
        .await
        .unwrap();
    let source = store.load_vendor_source(vendor).await.unwrap().unwrap();
    assert_eq!(source.aggregate.persisted_version, 1);
    assert_eq!(source.cell, position().obj_cell_id);
    assert_eq!(
        source.aggregate.bytes,
        vendor_create.inventory.snapshots[0].bytes
    );

    let marker = VendorStockSaveV1 {
        marker_object_id: marker_id,
        vendor_object_id: vendor,
        source_revision: 12,
        source_hash: [7; 32],
        loaded: true,
        stock_revision: 1,
        defaults: vec![VendorDefaultStockV1 {
            root: stock_id,
            display_quantity: -1,
            contribution_units: 0,
            child_ids: vec![],
        }],
        unique: vec![],
    };
    let mut operation = VendorStockOperation {
        world_epoch: owner.epoch(),
        vendor_expected_version: 1,
        inventory: PlacementOperation {
            operation_id: "vendor-stock-load-1".into(),
            snapshots: vec![snapshot(
                stock_id,
                item(
                    stock_id,
                    ItemPlacementV2::Contained {
                        container: vendor,
                        slot: 0,
                        pack_slot: false,
                        equipped: 0,
                    },
                ),
            )],
            participants: vec![vendor, marker_id, stock_id],
            leases: vec![],
            changes: vec![PlacementChange {
                item: stock_id,
                expected: None,
                destination: DurableItemPlace::Contained {
                    container: vendor,
                    slot: 0,
                    pack_slot: false,
                    equipped: 0,
                },
            }],
            storage_views: vec![],
        },
        marker: VendorStockWrite {
            marker_object_id: marker_id,
            expected_version: 0,
            expected_stock_revision: 0,
            mutation_revision: 1,
            bytes: marker.encode().unwrap(),
        },
    };
    let mut broken = operation.clone();
    let mut missing_child = marker.clone();
    missing_child.defaults[0].child_ids.push(0x8000_1403);
    broken.marker.bytes = missing_child.encode().unwrap();
    assert!(store.vendor_stock_operation(&broken).await.is_err());
    assert!(store.load_vendor_stock(vendor).await.unwrap().is_none());
    assert!(store.load(stock_id).await.unwrap().is_none());
    assert_eq!(store.item_place(stock_id).await.unwrap(), None);
    assert!(
        store
            .resolve_operation(&operation.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );

    let OperationOutcome::Committed(acks) = store.vendor_stock_operation(&operation).await.unwrap()
    else {
        panic!("first vendor load must commit");
    };
    assert_eq!(acks.len(), 2);
    assert!(
        acks.iter()
            .any(|ack| ack.object_id == stock_id && ack.persisted_version == 1)
    );
    assert!(
        acks.iter()
            .any(|ack| ack.object_id == marker_id && ack.persisted_version == 1)
    );
    assert_eq!(
        store
            .load_vendor_stock(vendor)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        marker.encode().unwrap()
    );
    assert_eq!(
        store.load(stock_id).await.unwrap().unwrap().bytes,
        operation.inventory.snapshots[0].bytes
    );
    let forest = store
        .load_vendor_stock_forest(vendor)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(forest.marker.marker_object_id, marker_id);
    assert_eq!(forest.items.len(), 1);
    assert_eq!(forest.items[0].aggregate.object_id, stock_id);
    assert_eq!(forest.items[0].aggregate.persisted_version, 1);
    assert_eq!(
        forest.items[0].placement,
        DurableItemPlace::Contained {
            container: vendor,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }
    );
    assert_eq!(
        store.vendor_stock_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut marker_only = operation.clone();
    marker_only.inventory.operation_id = "vendor-stock-marker-only".into();
    marker_only.inventory.snapshots.clear();
    marker_only.inventory.changes.clear();
    marker_only.marker.expected_version = 1;
    marker_only.marker.expected_stock_revision = 1;
    marker_only.marker.mutation_revision = 2;
    marker_only.marker.bytes = VendorStockSaveV1 {
        stock_revision: 2,
        ..marker.clone()
    }
    .encode()
    .unwrap();
    marker_only.vendor_expected_version = 2;
    assert!(matches!(
        store.vendor_stock_operation(&marker_only).await,
        Err(StoreError::Conflict(id)) if id == vendor
    ));
    assert!(
        store
            .resolve_operation(&marker_only.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    marker_only.vendor_expected_version = 1;
    let OperationOutcome::Committed(marker_acks) =
        store.vendor_stock_operation(&marker_only).await.unwrap()
    else {
        panic!("marker-only revision must commit");
    };
    assert_eq!(marker_acks.len(), 1);
    assert_eq!(marker_acks[0].object_id, marker_id);
    assert_eq!(marker_acks[0].persisted_version, 2);
    assert_eq!(
        store.vendor_stock_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    operation.marker.bytes = VendorStockSaveV1 {
        source_hash: [8; 32],
        ..marker
    }
    .encode()
    .unwrap();
    assert!(matches!(
        store.vendor_stock_operation(&operation).await,
        Err(StoreError::OperationMismatch)
    ));
    assert!(matches!(
        store
            .save_batch(&[snapshot(
                marker_id,
                item(marker_id, ItemPlacementV2::World(position())),
            )])
            .await,
        Err(StoreError::OwnershipConflict)
    ));
    owner.close().await.unwrap();
    store.close().await;
}
