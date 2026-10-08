use super::Cluster;
use bace_db_postgres::PgStore;
use bace_persistence::*;
use bace_storage_codec::*;

#[tokio::test]
async fn item_v4_construction_survives_reload_and_rejects_erasure_or_origin_change() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let id = 0x80000031;
    let mut saved = ItemSaveV4 {
        previous: ItemSaveV3::migrate_v2(ItemSaveV2 {
            entity: EntitySaveV1 {
                object_id: id,
                template_revision: 1,
                mutation_revision: 1,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 100,
                    class_name: "constructed_fixture".into(),
                    weenie_type: 10,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            placement: ItemPlacementV2::World(bace_content::Position {
                obj_cell_id: 0x12340001,
                position_x: 1.,
                position_y: 2.,
                position_z: 3.,
                rotation_w: 1.,
                rotation_x: 0.,
                rotation_y: 0.,
                rotation_z: 0.,
            }),
        })
        .unwrap(),
        construction: Some(FrozenCreatureConstructionV1 {
            weenie_type: 10,
            origin: FrozenGeneratorConstructionOriginV1 {
                generator: 77,
                incarnation: 1,
                content_revision: 1,
                profile: 0,
                occurrence: 0,
                random_identity: [9; 16],
                random_key_version: 1,
            },
            equipment_order: vec![],
            death_roster: vec![],
        }),
    };
    let snapshot = |saved: &ItemSaveV4, expected_version| SaveSnapshot {
        object_id: id,
        mutation_revision: saved.entity.mutation_revision,
        expected_version,
        bytes: saved.encode().unwrap(),
    };
    let mut operation = PlacementOperation {
        operation_id: "construction-create".into(),
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
    saved
        .entity
        .state
        .properties
        .strings
        .push(bace_content::Property {
            id: 1,
            value: "renamed".into(),
        });
    operation.operation_id = "construction-update".into();
    operation.changes.clear();
    operation.snapshots = vec![snapshot(&saved, 1)];
    store.placement_operation(&operation).await.unwrap();
    assert_eq!(
        store.placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    for index in 0..3 {
        let mut rejected = saved.clone();
        rejected.entity.mutation_revision = 3;
        match index {
            0 => rejected.construction = None,
            1 => rejected.construction.as_mut().unwrap().origin.occurrence += 1,
            _ => {
                rejected
                    .construction
                    .as_mut()
                    .unwrap()
                    .origin
                    .random_identity[0] ^= 1
            }
        }
        operation.operation_id = format!("construction-rejected-{index}");
        operation.snapshots = vec![snapshot(&rejected, 2)];
        assert!(store.placement_operation(&operation).await.is_err());
        assert!(
            store
                .resolve_operation(&operation.operation_id)
                .await
                .unwrap()
                .is_none()
        );
        let accepted = store.load(id).await.unwrap().unwrap();
        assert_eq!(accepted.persisted_version, 2);
        assert_eq!(ItemSaveV4::decode(&accepted.bytes).unwrap(), saved);
    }
    let roots = store
        .load_world_item_tree(0x12340001, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(
        ItemSaveV4::decode(&roots[0].aggregate.bytes).unwrap(),
        saved
    );
    store.close().await;
}
