use super::{Cluster, snapshot};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{
    InventoryLoadLimits, InventoryOperation, ItemLocation, ItemTransfer, OwnedSaveBatch,
    OwnershipState,
};

#[tokio::test]
async fn nested_inventory_load_is_fenced_bounded_and_routine_descendants_are_atomic() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let offline = store
        .create_owned_character(&snapshot(100, 0, b"player"))
        .await
        .unwrap();
    store
        .inventory_operation(&InventoryOperation {
            operation_id: "initial-bag-items".into(),
            snapshots: vec![
                snapshot(100, 1, b"player"),
                snapshot(101, 0, b"bag"),
                snapshot(102, 0, b"child"),
            ],
            leases: vec![offline],
            transfers: vec![
                ItemTransfer {
                    item: 101,
                    expected: None,
                    destination: Some(ItemLocation {
                        container: 100,
                        slot: 4,
                    }),
                },
                ItemTransfer {
                    item: 102,
                    expected: None,
                    destination: Some(ItemLocation {
                        container: 101,
                        slot: 7,
                    }),
                },
            ],
        })
        .await
        .unwrap();
    let load = store.begin_login(offline).await.unwrap();
    let inventory = store
        .load_character_inventory(
            load.lease,
            InventoryLoadLimits {
                max_items: 2,
                max_depth: 2,
                max_total_bytes: 8,
            },
        )
        .await
        .unwrap();
    assert_eq!(inventory.items.len(), 2);
    assert_eq!(inventory.items[0].aggregate.object_id, 101);
    assert_eq!(inventory.items[0].depth, 1);
    assert_eq!(inventory.items[1].aggregate.bytes, b"child");
    assert_eq!(inventory.items[1].depth, 2);
    assert_eq!(
        inventory.items[1].location,
        ItemLocation {
            container: 101,
            slot: 7
        }
    );
    for limits in [
        InventoryLoadLimits {
            max_items: 1,
            ..Default::default()
        },
        InventoryLoadLimits {
            max_depth: 1,
            ..Default::default()
        },
        InventoryLoadLimits {
            max_total_bytes: 7,
            ..Default::default()
        },
    ] {
        assert!(
            store
                .load_character_inventory(load.lease, limits)
                .await
                .is_err()
        );
    }
    assert!(
        store
            .load_character_inventory(offline, InventoryLoadLimits::default())
            .await
            .is_err()
    );
    let mut routine = OwnedSaveBatch {
        snapshots: vec![snapshot(102, 1, b"new-child")],
        participants: vec![100, 101, 102],
        leases: vec![offline],
    };
    assert!(store.save_owned_batch(&routine).await.is_err());
    let online = store.finish_login(load.lease).await.unwrap();
    routine.leases = vec![online];
    assert!(store.resolve_owned_batch(&routine).await.unwrap().is_none());
    let ack = store.save_owned_batch(&routine).await.unwrap();
    assert_eq!(
        store.resolve_owned_batch(&routine).await.unwrap().unwrap(),
        ack
    );
    let mut wrong = routine.clone();
    wrong.snapshots[0].bytes = b"different bytes".to_vec();
    assert!(store.resolve_owned_batch(&wrong).await.is_err());
    assert_eq!(ack[0].persisted_version, 2);
    assert_eq!(store.load(100).await.unwrap().unwrap().persisted_version, 2);
    assert_eq!(store.load(101).await.unwrap().unwrap().persisted_version, 1);
    routine.snapshots[0] = snapshot(102, 2, b"should-not-save");
    routine.participants = vec![100, 102];
    assert!(store.save_owned_batch(&routine).await.is_err());
    assert_eq!(store.load(102).await.unwrap().unwrap().bytes, b"new-child");
    routine.participants = vec![100, 101, 102];
    routine.leases.clear();
    assert!(matches!(
        store.save_owned_batch(&routine).await,
        Err(StoreError::OwnershipConflict)
    ));
    routine.leases = vec![online];
    routine.snapshots = vec![
        snapshot(101, 1, b"bag-rollback"),
        snapshot(102, 1, b"stale"),
    ];
    assert!(store.save_owned_batch(&routine).await.is_err());
    assert_eq!(store.load(101).await.unwrap().unwrap().bytes, b"bag");
    assert!(matches!(
        store.save_batch(&[snapshot(102, 2, b"bypass")]).await,
        Err(StoreError::OwnershipConflict)
    ));
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM durable_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(receipts, 1, "routine saves must not add operation receipts");
    pool.close().await;
    let draining = store.begin_logout(online).await.unwrap();
    routine.snapshots = vec![snapshot(102, 2, b"old-online-write")];
    assert!(store.save_owned_batch(&routine).await.is_err());
    assert_eq!(draining.state, OwnershipState::LoggingOut);
    store.close().await;
}

#[tokio::test]
async fn routine_housing_updates_require_owner_ancestor_and_preserve_relational_identity() {
    use bace_storage_codec::{EntitySaveV1, HouseSaveV1};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("routine-house-owner").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("fresh account")
    };
    let player = bace_storage_codec::PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "synthetic_player".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Synthetic Owner".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let offline = store.create_player(&player, 0, 11, &[]).await.unwrap();
    let mut house = HouseSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x80000001,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "house".into(),
                weenie_type: 53,
                last_modified: None,
                properties: Default::default(),
            },
        },
        house_id: 10,
        owner_id: offline.character_id,
        purchased_at: 1,
        rent_period_start: 1,
        rent_due_at: 2,
        access: vec![],
    };
    store
        .save_house("initial-house", offline, &house, 0)
        .await
        .unwrap();
    house.entity.mutation_revision = 2;
    let mut batch = OwnedSaveBatch {
        snapshots: vec![snapshot(
            house.entity.object_id,
            1,
            &house.encode().unwrap(),
        )],
        participants: vec![house.entity.object_id],
        leases: vec![],
    };
    assert!(store.save_owned_batch(&batch).await.is_err());
    batch.participants.push(offline.character_id);
    batch.leases.push(offline);
    assert_eq!(
        store.save_owned_batch(&batch).await.unwrap()[0].persisted_version,
        2
    );
    house.owner_id = 0x50000002;
    batch.snapshots = vec![snapshot(
        house.entity.object_id,
        2,
        &house.encode().unwrap(),
    )];
    assert!(matches!(
        store.save_owned_batch(&batch).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert_eq!(
        HouseSaveV1::decode(
            &store
                .load(house.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes
        )
        .unwrap()
        .owner_id,
        offline.character_id
    );
    store.close().await;
}

#[tokio::test]
async fn routine_resolution_waits_for_original_writer_before_deciding_commit() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let lease = store
        .create_owned_character(&snapshot(400, 0, b"before"))
        .await
        .unwrap();
    let batch = OwnedSaveBatch {
        snapshots: vec![snapshot(400, 1, b"after")],
        participants: vec![400],
        leases: vec![lease],
    };
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let mut original = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
        .execute(&mut *original)
        .await
        .unwrap();
    sqlx::query("UPDATE entity_snapshots SET version=2,payload=$1 WHERE object_id=400")
        .bind(b"after".as_slice())
        .execute(&mut *original)
        .await
        .unwrap();
    let resolver = store.clone();
    let request = batch.clone();
    let mut pending = tokio::spawn(async move { resolver.resolve_owned_batch(&request).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
            .await
            .is_err(),
        "uncommitted old row cannot resolve uncertain CAS"
    );
    original.commit().await.unwrap();
    let resolved = pending.await.unwrap().unwrap().unwrap();
    assert_eq!(resolved[0].persisted_version, 2);
    assert_eq!(
        resolved[0].mutation_revision,
        batch.snapshots[0].mutation_revision
    );
    assert!(
        store
            .resolve_owned_batch(&OwnedSaveBatch {
                leases: vec![bace_persistence::CharacterLease {
                    epoch: lease.epoch + 1,
                    ..lease
                }],
                ..batch
            })
            .await
            .is_err()
    );
    pool.close().await;
    store.close().await;
}
