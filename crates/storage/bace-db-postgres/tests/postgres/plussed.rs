//! ACE Shard.Character.IsPlussed is a persisted display flag, not account access.
use super::*;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};
#[tokio::test]
async fn plussed_is_independent_persisted_and_read_under_exact_account_lease() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("plussed-source").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "plussed_player".into(),
                weenie_type: 10,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Plussed Player".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let offline = store.create_player(&player, 0, 11, &[]).await.unwrap();
    assert!(!store.players_for_account(account.id.0).await.unwrap()[0].is_plussed);
    assert!(
        store
            .player_is_plussed(offline, account.id.0)
            .await
            .is_err()
    );
    let sql = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    sqlx::query("UPDATE players SET is_plussed=true WHERE object_id=$1")
        .bind(i64::from(player.entity.object_id))
        .execute(&sql)
        .await
        .unwrap();
    let loading = store.begin_login(offline).await.unwrap().lease;
    assert!(
        store
            .player_is_plussed(loading, account.id.0)
            .await
            .unwrap()
    );
    assert!(
        store
            .player_is_plussed(loading, account.id.0 + 1)
            .await
            .is_err()
    );
    let online = store.finish_login(loading).await.unwrap();
    assert!(
        store
            .player_is_plussed(loading, account.id.0)
            .await
            .is_err()
    );
    assert!(store.player_is_plussed(online, account.id.0).await.unwrap());
    assert_eq!(
        account.access_level,
        bace_auth::AccessLevel::Player,
        "a display plus never grants staff authority"
    );
    drop(store);
    let reopened = PgStore::connect(&cluster.url(), 2).await.unwrap();
    assert!(reopened.players_for_account(account.id.0).await.unwrap()[0].is_plussed);
    assert!(
        reopened
            .player_is_plussed(online, account.id.0)
            .await
            .unwrap()
    );
}
