use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::{PgStore, PlayerCreationConflict, StoreError};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};
#[tokio::test]
async fn creation_name_and_slot_conflicts_are_definite_and_leave_no_partial_rows() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("creation-conflict").unwrap(),
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
    let mut player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: store.allocate_player_id().await.unwrap(),
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "creation_fixture".into(),
                weenie_type: 10,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Creation Probe".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    store.create_player(&player, 0, 11, &[]).await.unwrap();
    player.entity.object_id = store.allocate_player_id().await.unwrap();
    let name = store.create_player(&player, 1, 11, &[]).await.unwrap_err();
    assert_eq!(
        name.player_creation_conflict(),
        Some(PlayerCreationConflict::Name)
    );
    assert!(store.load(player.entity.object_id).await.unwrap().is_none());
    player.name = "Different Probe".into();
    let slot = store.create_player(&player, 0, 11, &[]).await.unwrap_err();
    assert_eq!(
        slot.player_creation_conflict(),
        Some(PlayerCreationConflict::Slot)
    );
    assert!(store.load(player.entity.object_id).await.unwrap().is_none());
    // An uncertain acknowledgment never becomes a definite user rejection.
    let uncertain = StoreError::CommitUncertain(sqlx::Error::Io(std::io::Error::other(
        "lost commit acknowledgment",
    )));
    assert_eq!(uncertain.player_creation_conflict(), None);
    store.create_player(&player, 1, 11, &[]).await.unwrap();
    store.close().await;
}
