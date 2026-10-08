use super::{Cluster, snapshot};
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{InventoryOperation, OwnedSaveBatch};
use bace_storage_codec::{EntitySaveV1, HouseSaveV1, PlayerSaveV1};

#[tokio::test]
async fn physical_recovery_rejects_early_clear_and_schema_downgrade() {
    use bace_storage_codec::{PhysicalRecoverySaveV1, PlayerSaveV6};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (old, lease) = player(&store).await;
    let id = old.entity.object_id;
    let mut current = PlayerSaveV6::migrate_v1(old).unwrap();
    current.player.entity.mutation_revision = 2;
    current.physical_recovery = Some(PhysicalRecoverySaveV1 {
        captured_unix_millis: 10_000,
        remaining_seconds: 2.0,
    });
    let bytes = current.encode().unwrap();
    store
        .save_offline(lease, &snapshot(id, 1, &bytes))
        .await
        .unwrap();
    let mut early = current.clone();
    early.physical_recovery.as_mut().unwrap().remaining_seconds = 0.0;
    assert!(matches!(
        store
            .save_offline(lease, &snapshot(id, 2, &early.encode().unwrap()))
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert!(matches!(
        store
            .save_offline(lease, &snapshot(id, 2, &current.previous.encode().unwrap()))
            .await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(store.load(id).await.unwrap().unwrap().bytes, bytes);
    current.physical_recovery = Some(PhysicalRecoverySaveV1 {
        captured_unix_millis: 11_000,
        remaining_seconds: 1.0,
    });
    store
        .save_offline(lease, &snapshot(id, 2, &current.encode().unwrap()))
        .await
        .unwrap();
    store.close().await;
}

async fn player(store: &PgStore) -> (PlayerSaveV1, bace_persistence::CharacterLease) {
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("identity-owner").unwrap(),
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
    let state = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "identity_player".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state,
        },
        account_id: account.id.0,
        name: "Identity Owner".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&player, 0, 11, &[]).await.unwrap();
    (player, lease)
}
#[tokio::test]
async fn all_registered_player_write_paths_preserve_relational_identity_and_failed_receipts_rollback()
 {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, offline) = player(&store).await;
    let original = player.encode().unwrap();
    for change in 0..3 {
        let mut invalid = player.clone();
        match change {
            0 => invalid.entity.object_id += 1,
            1 => invalid.account_id += 1,
            _ => invalid.name = "Different Owner".into(),
        };
        let bad = snapshot(player.entity.object_id, 1, &invalid.encode().unwrap());
        assert!(matches!(
            store.save_offline(offline, &bad).await,
            Err(StoreError::OwnershipConflict)
        ));
        assert!(matches!(
            store
                .save_owned_batch(&OwnedSaveBatch {
                    snapshots: vec![bad.clone()],
                    participants: vec![player.entity.object_id],
                    leases: vec![offline]
                })
                .await,
            Err(StoreError::OwnershipConflict)
        ));
        let operation = InventoryOperation {
            operation_id: format!("identity-rejected-{change}"),
            snapshots: vec![snapshot(77, 0, b"must roll back"), bad],
            leases: vec![offline],
            transfers: vec![],
        };
        assert!(matches!(
            store.inventory_operation(&operation).await,
            Err(StoreError::OwnershipConflict)
        ));
        assert!(
            store
                .resolve_operation(&operation.operation_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(store.load(77).await.unwrap().is_none());
        assert_eq!(
            store
                .load(player.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes,
            original
        );
    }
    assert!(matches!(
        store
            .save_offline(offline, &snapshot(player.entity.object_id, 1, b"opaque"))
            .await,
        Err(StoreError::Invalid(_))
    ));
    let online = store
        .finish_login(store.begin_login(offline).await.unwrap().lease)
        .await
        .unwrap();
    let mut bad_player = player.clone();
    bad_player.name = "Renamed outside identity transaction".into();
    let bad = snapshot(player.entity.object_id, 1, &bad_player.encode().unwrap());
    assert!(matches!(
        store.save_owned(online, &bad).await,
        Err(StoreError::OwnershipConflict)
    ));
    let draining = store.begin_logout(online).await.unwrap();
    assert!(matches!(
        store.finish_logout(draining, &bad).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert_eq!(
        store
            .character_lease(player.entity.object_id)
            .await
            .unwrap(),
        Some(draining)
    );
    store
        .finish_logout(draining, &snapshot(player.entity.object_id, 1, &original))
        .await
        .unwrap();
    // Foundation ownership rows have no PlayerSave schema promise and stay opaque.
    let foundation = store
        .create_owned_character(&snapshot(300, 0, b"foundation"))
        .await
        .unwrap();
    store
        .save_offline(foundation, &snapshot(300, 1, b"still opaque"))
        .await
        .unwrap();
    assert_eq!(
        store.load(300).await.unwrap().unwrap().bytes,
        b"still opaque"
    );
    store.close().await;
}
#[tokio::test]
async fn critical_inventory_cannot_change_house_identity_or_partially_save_its_owner() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, offline) = player(&store).await;
    let house = HouseSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x80000001,
            template_revision: 1,
            mutation_revision: 1,
            state: player.entity.state.clone(),
        },
        house_id: 10,
        owner_id: player.entity.object_id,
        purchased_at: 1,
        rent_period_start: 1,
        rent_due_at: 2,
        access: vec![],
    };
    store
        .save_house("initial-identity-house", offline, &house, 0)
        .await
        .unwrap();
    let original = house.encode().unwrap();
    for change in 0..3 {
        let mut invalid = house.clone();
        match change {
            0 => invalid.entity.object_id += 1,
            1 => invalid.owner_id += 1,
            _ => invalid.house_id += 1,
        };
        let operation = InventoryOperation {
            operation_id: format!("house-identity-rejected-{change}"),
            snapshots: vec![
                snapshot(player.entity.object_id, 1, &player.encode().unwrap()),
                snapshot(house.entity.object_id, 1, &invalid.encode().unwrap()),
            ],
            leases: vec![offline],
            transfers: vec![],
        };
        assert!(matches!(
            store.inventory_operation(&operation).await,
            Err(StoreError::OwnershipConflict)
        ));
        assert!(
            store
                .resolve_operation(&operation.operation_id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .load(player.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .persisted_version,
            1
        );
        assert_eq!(
            store
                .load(house.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes,
            original
        );
    }
    // The central gate must still permit legitimate house state and owner snapshots.
    let mut updated = house.clone();
    updated.entity.mutation_revision = 2;
    store
        .inventory_operation(&InventoryOperation {
            operation_id: "valid-house-state".into(),
            snapshots: vec![
                snapshot(player.entity.object_id, 1, &player.encode().unwrap()),
                snapshot(house.entity.object_id, 1, &updated.encode().unwrap()),
            ],
            leases: vec![offline],
            transfers: vec![],
        })
        .await
        .unwrap();
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
        .entity
        .mutation_revision,
        2
    );
    store.close().await;
}

#[tokio::test]
async fn player_v3_supplements_survive_durable_retry_and_schema_downgrade_is_atomic() {
    use bace_persistence::{OperationOutcome, PlacementOperation, SaveSnapshot};
    use bace_storage_codec::{FrozenEnchantmentV1, PlayerSaveV3};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (legacy, offline) = player(&store).await;
    let online = store
        .finish_login(store.begin_login(offline).await.unwrap().lease)
        .await
        .unwrap();
    let mut player = PlayerSaveV3::migrate_v1(legacy.clone()).unwrap();
    player.player.entity.mutation_revision = 2;
    player.ui.gameplay_options = vec![0, 255, 42];
    player.ui.spellbook_filters = 0x1234;
    player.player.metadata.options1 = 0xdeadbeef;
    player.enchantments.push(FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 12,
        spell_id: 100,
        layer_id: 3,
        has_spell_set_id: true,
        spell_category: 123,
        power_level: 200,
        start_time: -5.5,
        duration: 300.0,
        caster_object_id: 0x50000002,
        degrade_modifier: 0.125,
        degrade_limit: 0.25,
        last_time_degraded: -1.0,
        stat_mod_type: 0x02008000,
        stat_mod_key: 7,
        stat_mod_value: 10.0,
        spell_set_id: 9,
    });
    let bytes = player.encode().unwrap();
    let operation = PlacementOperation {
        operation_id: "v3-player-test-unique".into(),
        snapshots: vec![SaveSnapshot {
            object_id: legacy.entity.object_id,
            mutation_revision: 2,
            expected_version: 1,
            bytes: bytes.clone(),
        }],
        participants: vec![legacy.entity.object_id],
        leases: vec![online],
        changes: vec![],
        storage_views: vec![],
    };
    assert!(matches!(
        store.placement_operation(&operation).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let stored = store.load(legacy.entity.object_id).await.unwrap().unwrap();
    assert_eq!(stored.bytes, bytes);
    assert_eq!(PlayerSaveV3::decode(&stored.bytes).unwrap(), player);
    let mut wrong = operation.clone();
    wrong.snapshots[0].bytes = legacy.encode().unwrap();
    assert!(matches!(
        store.placement_operation(&wrong).await,
        Err(StoreError::OperationMismatch)
    ));
    wrong.operation_id = "v3-player-downgrade-rejected".into();
    wrong.snapshots[0].expected_version = stored.persisted_version;
    assert!(matches!(
        store.placement_operation(&wrong).await,
        Err(StoreError::Invalid(_))
    ));
    assert!(
        store
            .resolve_operation(&wrong.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .load(legacy.entity.object_id)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        bytes
    );
    store.close().await;
}
