use super::Cluster;
use bace_db_postgres::PgStore;
use bace_persistence::*;
use bace_storage_codec::*;

#[tokio::test]
async fn item_source_destination_survives_save_and_cannot_be_rewritten() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let id = 0x80000032;
    let position = bace_content::Position {
        obj_cell_id: 0x12340001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    };
    let mut saved = ItemSaveV5 {
        previous: ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity: EntitySaveV1 {
                object_id: id,
                template_revision: 1,
                mutation_revision: 1,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 100,
                    class_name: "source_origin_fixture".into(),
                    weenie_type: 2,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            placement: ItemPlacementV2::World(position),
        })
        .unwrap(),
        source_destination: Some(9), // Contain | Treasure in pinned ACE.
    };
    let snapshot = |value: &ItemSaveV5, expected_version| SaveSnapshot {
        object_id: id,
        mutation_revision: value.entity.mutation_revision,
        expected_version,
        bytes: value.encode().unwrap(),
    };
    let mut operation = PlacementOperation {
        operation_id: "origin-create".into(),
        snapshots: vec![snapshot(&saved, 0)],
        participants: vec![id],
        leases: vec![],
        storage_views: vec![],
        changes: vec![PlacementChange {
            item: id,
            expected: None,
            destination: DurableItemPlace::World { cell: 0x12340001 },
        }],
    };
    store.placement_operation(&operation).await.unwrap();
    saved.entity.mutation_revision = 2;
    operation.operation_id = "origin-update".into();
    operation.changes.clear();
    operation.snapshots = vec![snapshot(&saved, 1)];
    store.placement_operation(&operation).await.unwrap();
    assert_eq!(
        store.placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    for (index, origin) in [None, Some(8), Some(10)].into_iter().enumerate() {
        let mut rejected = saved.clone();
        rejected.entity.mutation_revision = 3;
        rejected.source_destination = origin;
        operation.operation_id = format!("origin-rejected-{index}");
        operation.snapshots = vec![snapshot(&rejected, 2)];
        assert!(store.placement_operation(&operation).await.is_err());
        assert_eq!(
            ItemSaveV5::decode(&store.load(id).await.unwrap().unwrap().bytes).unwrap(),
            saved
        );
    }
    let roots = store
        .load_world_item_tree(0x12340001, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(
        ItemSaveV5::decode(&roots[0].aggregate.bytes).unwrap(),
        saved
    );
    store.close().await;
}
