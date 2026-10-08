use super::{Cluster, snapshot};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::OperationOutcome;

#[tokio::test]
async fn inventory_transfers_are_fenced_atomic_and_replay_safe() {
    use bace_persistence::{InventoryOperation, ItemLocation, ItemTransfer, OwnershipState};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let offline = store
        .create_owned_character(&snapshot(100, 0, b"player"))
        .await
        .unwrap();
    let loading = store.begin_login(offline).await.unwrap();
    let online = store.finish_login(loading.lease).await.unwrap();
    let bag = ItemLocation {
        container: 100,
        slot: 0,
    };
    let creation = InventoryOperation {
        operation_id: "loot-create".into(),
        snapshots: vec![
            snapshot(100, 1, b"player-with-item"),
            snapshot(101, 0, b"item"),
        ],
        leases: vec![online],
        transfers: vec![ItemTransfer {
            item: 101,
            expected: None,
            destination: Some(bag),
        }],
    };
    assert!(matches!(
        store.inventory_operation(&creation).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.inventory_operation(&creation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    assert_eq!(store.item_location(101).await.unwrap(), Some(bag));
    // Ordinary writers must not bypass character fencing through an owned item.
    assert!(matches!(
        store.save_batch(&[snapshot(101, 1, b"bypass")]).await,
        Err(StoreError::OwnershipConflict)
    ));
    let mut missing_lease = creation.clone();
    missing_lease.operation_id = "missing-lease".into();
    missing_lease.leases.clear();
    missing_lease.snapshots = vec![snapshot(100, 2, b"removed"), snapshot(101, 1, b"dropped")];
    missing_lease.transfers[0] = ItemTransfer {
        item: 101,
        expected: Some(bag),
        destination: None,
    };
    assert!(matches!(
        store.inventory_operation(&missing_lease).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(
        store
            .resolve_operation("missing-lease")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.load(100).await.unwrap().unwrap().bytes,
        b"player-with-item"
    );
    let collision = InventoryOperation {
        operation_id: "occupied-slot".into(),
        snapshots: vec![snapshot(100, 2, b"invalid"), snapshot(102, 0, b"second")],
        leases: vec![online],
        transfers: vec![ItemTransfer {
            item: 102,
            expected: None,
            destination: Some(bag),
        }],
    };
    assert!(store.inventory_operation(&collision).await.is_err());
    assert!(store.load(102).await.unwrap().is_none());
    assert!(
        store
            .resolve_operation("occupied-slot")
            .await
            .unwrap()
            .is_none()
    );
    // A second character competes for the same item; only one stale proposal wins.
    let second = store
        .create_owned_character(&snapshot(200, 0, b"other"))
        .await
        .unwrap();
    let second = store.begin_login(second).await.unwrap();
    let second = store.finish_login(second.lease).await.unwrap();
    let give = InventoryOperation {
        operation_id: "give".into(),
        snapshots: vec![
            snapshot(100, 2, b"given"),
            snapshot(200, 1, b"received"),
            snapshot(101, 1, b"moved"),
        ],
        leases: vec![online, second],
        transfers: vec![ItemTransfer {
            item: 101,
            expected: Some(bag),
            destination: Some(ItemLocation {
                container: 200,
                slot: 1,
            }),
        }],
    };
    let mut competitor = give.clone();
    competitor.operation_id = "competing-give".into();
    let (a, b) = tokio::join!(
        store.inventory_operation(&give),
        store.inventory_operation(&competitor)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let committed = if a.is_ok() { &give } else { &competitor };
    assert_eq!(
        store.inventory_operation(committed).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut mismatch = committed.clone();
    mismatch.snapshots[0].bytes = b"different".to_vec();
    assert!(matches!(
        store.inventory_operation(&mismatch).await,
        Err(StoreError::OperationMismatch)
    ));
    // Fencing applies before mutations, while acknowledged old receipts stay replayable.
    let logging_out = store.begin_logout(second).await.unwrap();
    assert_eq!(logging_out.state, OwnershipState::LoggingOut);
    let stale = InventoryOperation {
        operation_id: "stale-owner".into(),
        snapshots: vec![snapshot(200, 2, b"stale"), snapshot(101, 2, b"stale")],
        leases: vec![second],
        transfers: vec![ItemTransfer {
            item: 101,
            expected: Some(ItemLocation {
                container: 200,
                slot: 1,
            }),
            destination: None,
        }],
    };
    assert!(matches!(
        store.inventory_operation(&stale).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert_eq!(store.load(101).await.unwrap().unwrap().bytes, b"moved");
    store.close().await;
}

#[tokio::test]
async fn inventory_cycle_and_unreserved_ancestor_roll_back() {
    use bace_persistence::{InventoryOperation, ItemLocation, ItemTransfer};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let cycle = InventoryOperation {
        operation_id: "cycle".into(),
        snapshots: vec![snapshot(10, 0, b"bag-a"), snapshot(11, 0, b"bag-b")],
        leases: vec![],
        transfers: vec![
            ItemTransfer {
                item: 10,
                expected: None,
                destination: Some(ItemLocation {
                    container: 11,
                    slot: 0,
                }),
            },
            ItemTransfer {
                item: 11,
                expected: None,
                destination: Some(ItemLocation {
                    container: 10,
                    slot: 0,
                }),
            },
        ],
    };
    assert!(store.inventory_operation(&cycle).await.is_err());
    assert!(store.load(10).await.unwrap().is_none());
    assert!(store.resolve_operation("cycle").await.unwrap().is_none());
    let mut valid = cycle.clone();
    valid.operation_id = "nested".into();
    valid.transfers.truncate(1);
    store.inventory_operation(&valid).await.unwrap();
    let unreserved = InventoryOperation {
        operation_id: "unreserved".into(),
        snapshots: vec![snapshot(10, 1, b"bypass")],
        leases: vec![],
        transfers: vec![],
    };
    assert!(store.inventory_operation(&unreserved).await.is_err());
    assert_eq!(store.load(10).await.unwrap().unwrap().bytes, b"bag-a");
    store.close().await;
}

#[tokio::test]
async fn player_offline_housing_and_item_saves_survive_reconnect() {
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    use bace_persistence::{InventoryOperation, ItemLocation, ItemTransfer};
    use bace_storage_codec::{EntitySaveV1, HouseAccessV1, HouseSaveV1, PlayerSaveV1};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let account = store
        .create(NewAccount {
            name: AccountName::parse("save-test").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"local-test-password")
                .unwrap(),
        })
        .await
        .unwrap();
    let CreateAccountOutcome::Created(account) = account else {
        panic!("fresh account")
    };
    let id = store.allocate_player_id().await.unwrap();
    let template = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "player".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    let mut player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: template.clone(),
        },
        account_id: account.id.0,
        name: "Persistent Player".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let item = EntitySaveV1 {
        object_id: 0x8000_0001,
        template_revision: 1,
        mutation_revision: 1,
        state: template.clone(),
    };
    let mut invalid_item = item.clone();
    invalid_item.object_id = u32::MAX;
    assert!(
        store
            .create_player(&player, 0, 11, &[(invalid_item, 0)])
            .await
            .is_err()
    );
    assert!(store.load(id).await.unwrap().is_none());
    assert!(
        store
            .resolve_operation(&format!("character-create:{id}"))
            .await
            .unwrap()
            .is_none()
    );
    let lease = store
        .create_player(&player, 0, 11, &[(item.clone(), 0)])
        .await
        .unwrap();
    assert!(
        store
            .resolve_operation(&format!("character-create:{id}"))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        store
            .create_player(&player, 0, 11, &[(item.clone(), 0)])
            .await
            .unwrap(),
        lease
    );
    let mut reused = player.clone();
    reused.name = "Different Request".into();
    assert!(matches!(
        store
            .create_player(&reused, 0, 11, &[(item.clone(), 0)])
            .await,
        Err(StoreError::OperationMismatch)
    ));
    assert!(store.account_owns_player(account.id.0, id).await.unwrap());
    assert!(
        !store
            .account_owns_player(account.id.0 + 1, id)
            .await
            .unwrap()
    );
    assert_eq!(
        store.players_for_account(account.id.0).await.unwrap()[0].name,
        "Persistent Player"
    );
    assert_eq!(
        store.item_location(item.object_id).await.unwrap(),
        Some(ItemLocation {
            container: id,
            slot: 0
        })
    );
    let mut duplicate = player.clone();
    duplicate.entity.object_id = store.allocate_player_id().await.unwrap();
    assert!(store.create_player(&duplicate, 1, 11, &[]).await.is_err());
    assert!(
        store
            .load(duplicate.entity.object_id)
            .await
            .unwrap()
            .is_none()
    );
    player.entity.mutation_revision = 2;
    let offline_save = snapshot(id, 1, &player.encode().unwrap());
    store.save_offline(lease, &offline_save).await.unwrap();
    let house = HouseSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x7000_1000,
            template_revision: 1,
            mutation_revision: 1,
            state: template,
        },
        house_id: 42,
        owner_id: id,
        purchased_at: 100,
        rent_period_start: 100,
        rent_due_at: 200,
        access: vec![HouseAccessV1 {
            player_id: id,
            permissions: 3,
        }],
    };
    assert!(matches!(
        store
            .save_house("house-save", lease, &house, 0)
            .await
            .unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store
            .save_house("house-save", lease, &house, 0)
            .await
            .unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    assert!(matches!(
        store
            .save_batch(&[snapshot(house.entity.object_id, 1, b"bypass")])
            .await,
        Err(StoreError::OwnershipConflict)
    ));
    // Move an offline player's item into housing in one fenced commit.
    let transfer = InventoryOperation {
        operation_id: "offline-house-storage".into(),
        leases: vec![lease],
        snapshots: vec![
            snapshot(id, 2, &player.encode().unwrap()),
            snapshot(item.object_id, 1, &item.encode_item().unwrap()),
            snapshot(house.entity.object_id, 1, &house.encode().unwrap()),
        ],
        transfers: vec![ItemTransfer {
            item: item.object_id,
            expected: Some(ItemLocation {
                container: id,
                slot: 0,
            }),
            destination: Some(ItemLocation {
                container: house.entity.object_id,
                slot: 5,
            }),
        }],
    };
    store.inventory_operation(&transfer).await.unwrap();
    let loaded = store.begin_login(lease).await.unwrap();
    assert_eq!(
        PlayerSaveV1::decode(&loaded.snapshot.bytes).unwrap(),
        player
    );
    assert!(matches!(
        store
            .save_offline(lease, &snapshot(id, 3, &player.encode().unwrap()))
            .await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(matches!(
        store.save_house("stale-house", lease, &house, 2).await,
        Err(StoreError::OwnershipConflict)
    ));
    store.close().await;
    let reopened = PgStore::connect(&cluster.url(), 2).await.unwrap();
    assert_eq!(
        HouseSaveV1::decode(
            &reopened
                .load(house.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes
        )
        .unwrap(),
        house
    );
    assert_eq!(
        reopened.item_location(item.object_id).await.unwrap(),
        Some(ItemLocation {
            container: house.entity.object_id,
            slot: 5
        })
    );
    reopened.close().await;
}

#[tokio::test]
async fn exclusive_world_restart_fences_old_online_and_offline_leases() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let offline = store
        .create_owned_character(&snapshot(10, 0, b"durable"))
        .await
        .unwrap();
    let second = store
        .create_owned_character(&snapshot(11, 0, b"online-durable"))
        .await
        .unwrap();
    let loading = store.begin_login(second).await.unwrap();
    let online = store.finish_login(loading.lease).await.unwrap();
    let mut owner = store.acquire_world_owner().await.unwrap();
    assert!(matches!(
        store.acquire_world_owner().await,
        Err(StoreError::OwnershipConflict)
    ));
    owner.check().await.unwrap();
    assert_eq!(owner.recover_characters().await.unwrap(), 2);
    assert!(matches!(
        store
            .save_offline(offline, &snapshot(10, 1, b"stale"))
            .await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(matches!(
        store.save_owned(online, &snapshot(11, 1, b"stale")).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert_eq!(
        store.load(11).await.unwrap().unwrap().bytes,
        b"online-durable"
    );
    owner.close().await.unwrap();
    let mut successor = store.acquire_world_owner().await.unwrap();
    successor.check().await.unwrap();
    successor.close().await.unwrap();
    store.close().await;
}

#[tokio::test]
async fn dynamic_ids_are_bounded_unique_and_do_not_recycle() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    assert!(store.allocate_dynamic_ids(0).await.is_err());
    assert!(store.allocate_dynamic_ids(1025).await.is_err());
    let (a, b) = tokio::join!(
        store.allocate_dynamic_ids(32),
        store.allocate_dynamic_ids(32)
    );
    let mut values = a.unwrap();
    values.extend(b.unwrap());
    values.sort_unstable();
    values.dedup();
    assert_eq!(values.len(), 64);
    assert_eq!(values[0], 0x80000000);
    assert_eq!(values[63], 0x8000003f);
    assert_eq!(
        store.allocate_dynamic_ids(1).await.unwrap(),
        vec![0x80000040]
    );
    store.close().await;
}
