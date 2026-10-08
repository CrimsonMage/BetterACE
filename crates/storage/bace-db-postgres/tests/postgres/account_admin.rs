use super::*;
use bace_auth::{
    AccessLevel, AccountAdminChange, AccountAdminOperation, AccountAdminRepository, AccountName,
    AccountRepository, NewAccount, PasswordService,
};
#[tokio::test]
async fn staff_accounts_commit_cas_replay_and_rollback_without_plaintext() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let passwords = PasswordService::new(1).unwrap();
    let name = AccountName::parse("Rune").unwrap();
    let hash = passwords.hash(b"first-secret").unwrap();
    let create = AccountAdminOperation {
        operation_id: [1; 16],
        issuer: None,
        change: AccountAdminChange::Create {
            name: name.clone(),
            password_hash: hash.clone(),
            access: AccessLevel::Admin,
        },
    };
    let created = store.apply_account_admin(&create).await.unwrap();
    assert_eq!(created.revision, 1);
    assert_eq!(created.access, AccessLevel::Admin);
    assert_eq!(store.apply_account_admin(&create).await.unwrap(), created);
    let update = AccountAdminOperation {
        operation_id: [2; 16],
        issuer: Some(created.account),
        change: AccountAdminChange::Update {
            account: created.account,
            expected_revision: 1,
            access: Some(AccessLevel::Sentinel),
            password_hash: Some(passwords.hash(b"new-secret").unwrap()),
        },
    };
    let changed = store.apply_account_admin(&update).await.unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.access, AccessLevel::Sentinel);
    // Replay is the original committed result even after subsequent modifications.
    assert_eq!(store.apply_account_admin(&create).await.unwrap(), created);
    assert_eq!(store.apply_account_admin(&update).await.unwrap(), changed);
    let mut mismatch = update.clone();
    mismatch.change = AccountAdminChange::Update {
        account: created.account,
        expected_revision: 1,
        access: Some(AccessLevel::Player),
        password_hash: None,
    };
    assert!(matches!(
        store.apply_account_admin(&mismatch).await,
        Err(StoreError::OperationMismatch)
    ));
    let mut stale = update.clone();
    stale.operation_id = [3; 16];
    assert!(matches!(
        store.apply_account_admin(&stale).await,
        Err(StoreError::OwnershipConflict)
    ));
    let current = store.account_for_admin(&name).await.unwrap().unwrap();
    assert_eq!(current.revision, 2);
    assert!(
        passwords
            .verify(b"new-secret", &current.account.password_hash)
            .unwrap()
    );
    assert!(
        !passwords
            .verify(b"first-secret", &current.account.password_hash)
            .unwrap()
    );
    // A rejected stale transaction never journals a receipt: reuse its ID with a
    // freshly prepared revision succeeds (the failed transaction rolled back).
    stale.change = AccountAdminChange::Update {
        account: created.account,
        expected_revision: 2,
        access: Some(AccessLevel::Player),
        password_hash: None,
    };
    let third = store.apply_account_admin(&stale).await.unwrap();
    assert_eq!(third.revision, 3);
    store.close().await;
    let reopened = PgStore::connect(&cluster.url(), 2).await.unwrap();
    assert_eq!(
        reopened.apply_account_admin(&update).await.unwrap(),
        changed
    );
    let ordinary = AccountName::parse("ordinary").unwrap();
    reopened
        .create(NewAccount {
            name: ordinary.clone(),
            password_hash: hash,
        })
        .await
        .unwrap();
    let ordinary = reopened
        .account_for_admin(&ordinary)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ordinary.revision, 1);
    assert_eq!(ordinary.account.access_level, AccessLevel::Player);
    reopened.close().await;
}

#[tokio::test]
async fn account_creation_migration_preserves_unknown_history_and_dates_new_accounts() {
    let cluster = Cluster::start();
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0002_fresh_accounts.sql"))
        .execute(&raw)
        .await
        .unwrap();
    let hash = PasswordService::new(1).unwrap().hash(b"secret").unwrap();
    let old: i64 = sqlx::query_scalar(
        "INSERT INTO accounts(canonical_name,password_phc) VALUES('historical',$1) RETURNING id",
    )
    .bind(hash.as_phc())
    .fetch_one(&raw)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../migrations/0016_account_creation_time.sql"
    ))
    .execute(&raw)
    .await
    .unwrap();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    assert_eq!(
        store
            .account_creation_time(bace_types::AccountId(old as u64))
            .await
            .unwrap(),
        None
    );
    let before: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM CURRENT_TIMESTAMP))::bigint")
            .fetch_one(&raw)
            .await
            .unwrap();
    let created = store
        .create(NewAccount {
            name: AccountName::parse("newplayer").unwrap(),
            password_hash: hash,
        })
        .await
        .unwrap();
    let bace_auth::CreateAccountOutcome::Created(created) = created else {
        panic!("fresh account unexpectedly exists")
    };
    let stamp = store
        .account_creation_time(created.id)
        .await
        .unwrap()
        .unwrap();
    let after: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM CURRENT_TIMESTAMP))::bigint")
            .fetch_one(&raw)
            .await
            .unwrap();
    assert!((before..=after).contains(&stamp));
    assert!(
        store
            .account_creation_time(bace_types::AccountId(0))
            .await
            .is_err()
    );
    assert!(
        store
            .account_creation_time(bace_types::AccountId(u64::MAX))
            .await
            .is_err()
    );
    assert!(
        store
            .account_creation_time(bace_types::AccountId(9999))
            .await
            .is_err()
    );
    raw.close().await;
    store.close().await;
}
