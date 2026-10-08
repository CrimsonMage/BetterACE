use super::{Cluster, PgStore, StoreError};
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_persistence::*;
use bace_storage_codec::*;
async fn fixture(store: &PgStore) -> (PlayerSaveV6, CharacterLease) {
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("gagfixture").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"fixture-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("account fixture")
    };
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: store.allocate_player_id().await.unwrap(),
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "gag_fixture".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Gag Fixture".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&player, 0, 11, &[]).await.unwrap();
    (PlayerSaveV6::migrate_v1(player).unwrap(), lease)
}
fn operation(
    before: &PlayerSaveV6,
    lease: CharacterLease,
    version: i64,
    id: u8,
    enabled: bool,
) -> StaffGagOperation {
    let mut after = before.clone();
    after.player.entity.mutation_revision += 1;
    let props = &mut after.player.entity.state.properties;
    props.bools.retain(|p| p.id != 111);
    props.floats.retain(|p| ![112, 161].contains(&p.id));
    if enabled {
        props.bools.push(bace_content::Property {
            id: 111,
            value: true,
        });
        props.floats.extend([
            bace_content::Property {
                id: 112,
                value: 123.,
            },
            bace_content::Property {
                id: 161,
                value: 300.,
            },
        ]);
        props.floats.sort_by_key(|p| p.id);
    }
    StaffGagOperation {
        operation_id: [id; 16],
        issuer_account: before.player.account_id,
        lease,
        enabled,
        unix_seconds: 123.,
        snapshot: SaveSnapshot {
            object_id: lease.character_id,
            mutation_revision: after.player.entity.mutation_revision,
            expected_version: version,
            bytes: after.encode().unwrap(),
        },
    }
}
#[tokio::test]
async fn staff_gag_journal_fences_offline_login_and_replays_exact_receipt_after_online() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, offline) = fixture(&store).await;
    let loaded = store
        .load_offline_staff_player(offline.character_id)
        .await
        .unwrap();
    assert_eq!(loaded.lease, offline);
    let gag = operation(&player, offline, 1, 1, true);
    let receipt = store.apply_staff_gag(&gag).await.unwrap();
    assert_eq!(receipt.acknowledgement.persisted_version, 2);
    let loading = store.begin_login(offline).await.unwrap();
    assert_eq!(store.apply_staff_gag(&gag).await.unwrap(), receipt);
    assert!(matches!(
        store.load_offline_staff_player(offline.character_id).await,
        Err(StoreError::OwnershipConflict)
    ));
    let saved = PlayerSaveV6::decode(&loading.snapshot.bytes).unwrap();
    let stale = operation(&saved, offline, 2, 2, false);
    assert!(matches!(
        store.apply_staff_gag(&stale).await,
        Err(StoreError::OwnershipConflict)
    ));
    let mut reused = gag.clone();
    reused.unix_seconds = 124.;
    assert!(matches!(
        store.apply_staff_gag(&reused).await,
        Err(StoreError::OperationMismatch)
    ));
    let online = store.finish_login(loading.lease).await.unwrap();
    let ungag = operation(&saved, online, 2, 3, false);
    let receipt = store.apply_staff_gag(&ungag).await.unwrap();
    assert_eq!(receipt.acknowledgement.persisted_version, 3);
    assert_eq!(store.apply_staff_gag(&ungag).await.unwrap(), receipt);
    let stored = store.load(offline.character_id).await.unwrap().unwrap();
    assert_eq!(stored.bytes, ungag.snapshot.bytes);
    store.close().await;
}
#[tokio::test]
async fn staff_gag_offline_scope_and_stale_snapshot_fail_without_receipt_or_partial_write() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, lease) = fixture(&store).await;
    let valid = operation(&player, lease, 1, 4, true);
    let mut changed = valid.clone();
    let mut decoded = PlayerSaveV6::decode(&changed.snapshot.bytes).unwrap();
    decoded
        .player
        .entity
        .state
        .properties
        .ints
        .push(bace_content::Property { id: 77, value: 99 });
    changed.snapshot.bytes = decoded.encode().unwrap();
    assert!(matches!(
        store.apply_staff_gag(&changed).await,
        Err(StoreError::Invalid("staff offline mutation scope"))
    ));
    assert_eq!(
        store
            .load(lease.character_id)
            .await
            .unwrap()
            .unwrap()
            .persisted_version,
        1
    );
    let mut stale = valid.clone();
    stale.snapshot.expected_version = 2;
    assert!(matches!(
        store.apply_staff_gag(&stale).await,
        Err(StoreError::Conflict(_))
    ));
    let receipt = store.apply_staff_gag(&valid).await.unwrap();
    assert_eq!(receipt.acknowledgement.persisted_version, 2);
    assert!(matches!(
        store.apply_staff_gag(&changed).await,
        Err(StoreError::OperationMismatch)
    ));
    store.close().await;
}
