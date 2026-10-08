use super::*;
use bace_auth::{
    AccessLevel, AccountAdminChange, AccountAdminOperation, AccountAdminRepository, AccountName,
    AccountRepository, NewAccount, PasswordService,
};
use bace_persistence::{AccountBanChange, AccountBanOperation, AccountBanVerdict};

async fn account(store: &PgStore, name: &str, password: &bace_auth::PasswordHashRecord) -> u64 {
    let bace_auth::CreateAccountOutcome::Created(created) = store
        .create(NewAccount {
            name: AccountName::parse(name).unwrap(),
            password_hash: password.clone(),
        })
        .await
        .unwrap()
    else {
        panic!("new account already exists")
    };
    created.id.0
}

#[tokio::test]
async fn bans_commit_replay_cas_and_expire_before_login() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let password = PasswordService::new(1).unwrap().hash(b"secret").unwrap();
    let issuer = account(&store, "issuer", &password).await;
    let target = account(&store, "target", &password).await;
    assert_eq!(
        store.account_ban_verdict(target, 1000).await.unwrap(),
        AccountBanVerdict::Allowed
    );
    assert_eq!(
        store.account_ban_verdict(99999, 1000).await.unwrap(),
        AccountBanVerdict::Missing
    );
    let ban = AccountBanOperation {
        operation_id: [1; 16],
        account_id: target,
        expected_revision: 1,
        issuer_account_id: Some(issuer),
        change: AccountBanChange::Ban {
            started_unix_millis: 1000,
            expires_unix_millis: 2000,
            reason: Some("reason one".into()),
        },
    };
    let committed = store.apply_account_ban(&ban).await.unwrap();
    assert_eq!(committed.account_revision, 2);
    let record = committed.ban.as_ref().unwrap();
    assert_eq!(record.started_unix_millis, 1000);
    assert_eq!(record.expires_unix_millis, 2000);
    assert_eq!(record.issuer_account_id, Some(issuer));
    assert_eq!(record.reason.as_deref(), Some("reason one"));
    assert_eq!(store.apply_account_ban(&ban).await.unwrap(), committed);
    assert!(matches!(
        store.account_ban_verdict(target, 1999).await.unwrap(),
        AccountBanVerdict::Banned(_)
    ));
    assert_eq!(
        store.list_active_account_bans(1999, None, 1).await.unwrap()[0]
            .issuer_canonical_name
            .as_deref(),
        Some("issuer")
    );
    assert!(
        store
            .list_active_account_bans(2000, None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let AccountBanVerdict::Expired(expired) =
        store.account_ban_verdict(target, 2000).await.unwrap()
    else {
        panic!("expiry boundary must remain explicit")
    };
    assert_eq!(expired.account_revision, 2);
    let too_early = AccountBanOperation {
        operation_id: [2; 16],
        account_id: target,
        expected_revision: 2,
        issuer_account_id: None,
        change: AccountBanChange::Expire {
            now_unix_millis: 1999,
        },
    };
    assert!(matches!(
        store.apply_account_ban(&too_early).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(matches!(
        store.account_ban_verdict(target, 1999).await.unwrap(),
        AccountBanVerdict::Banned(_)
    ));
    let expire = AccountBanOperation {
        operation_id: [3; 16],
        change: AccountBanChange::Expire {
            now_unix_millis: 2000,
        },
        ..too_early
    };
    let cleared = store.apply_account_ban(&expire).await.unwrap();
    assert_eq!(cleared.account_revision, 3);
    assert!(cleared.ban.is_none());
    assert_eq!(store.apply_account_ban(&expire).await.unwrap(), cleared);
    assert_eq!(
        store.account_ban_verdict(target, 2000).await.unwrap(),
        AccountBanVerdict::Allowed
    );
    assert_eq!(store.apply_account_ban(&ban).await.unwrap(), committed);
    let mut mismatch = expire.clone();
    mismatch.change = AccountBanChange::Unban;
    assert!(matches!(
        store.apply_account_ban(&mismatch).await,
        Err(StoreError::OperationMismatch)
    ));
    store.close().await;
}

#[tokio::test]
async fn reban_reason_staff_revision_and_failed_mutations_are_atomic() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let password = PasswordService::new(1).unwrap().hash(b"secret").unwrap();
    let target = account(&store, "alpha", &password).await;
    let second = account(&store, "beta", &password).await;
    let first = AccountBanOperation {
        operation_id: [10; 16],
        account_id: target,
        expected_revision: 1,
        issuer_account_id: None,
        change: AccountBanChange::Ban {
            started_unix_millis: 100,
            expires_unix_millis: 200,
            reason: Some("retained".into()),
        },
    };
    store.apply_account_ban(&first).await.unwrap();
    let reban = AccountBanOperation {
        operation_id: [11; 16],
        expected_revision: 2,
        change: AccountBanChange::Ban {
            started_unix_millis: 150,
            expires_unix_millis: 250,
            reason: None,
        },
        ..first.clone()
    };
    let rebanned = store.apply_account_ban(&reban).await.unwrap();
    assert_eq!(
        rebanned.ban.as_ref().unwrap().reason.as_deref(),
        Some("retained")
    );
    let stale = AccountBanOperation {
        operation_id: [12; 16],
        expected_revision: 2,
        change: AccountBanChange::Unban,
        ..first.clone()
    };
    assert!(matches!(
        store.apply_account_ban(&stale).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert_eq!(
        store
            .account_for_admin(&AccountName::parse("alpha").unwrap())
            .await
            .unwrap()
            .unwrap()
            .revision,
        3
    );
    let admin = AccountAdminOperation {
        operation_id: [13; 16],
        issuer: None,
        change: AccountAdminChange::Update {
            account: bace_types::AccountId(target),
            expected_revision: 3,
            access: Some(AccessLevel::Sentinel),
            password_hash: None,
        },
    };
    store.apply_account_admin(&admin).await.unwrap();
    let unban = AccountBanOperation {
        operation_id: [14; 16],
        expected_revision: 4,
        ..stale
    };
    let unbanned = store.apply_account_ban(&unban).await.unwrap();
    assert_eq!(unbanned.account_revision, 5);
    assert!(unbanned.ban.is_none());
    assert_eq!(store.apply_account_ban(&unban).await.unwrap(), unbanned);
    let cannot_unban = AccountBanOperation {
        operation_id: [15; 16],
        expected_revision: 5,
        ..unban
    };
    assert!(matches!(
        store.apply_account_ban(&cannot_unban).await,
        Err(StoreError::OwnershipConflict)
    ));
    let second_ban = AccountBanOperation {
        operation_id: [16; 16],
        account_id: second,
        expected_revision: 1,
        issuer_account_id: None,
        change: AccountBanChange::Ban {
            started_unix_millis: 100,
            expires_unix_millis: 300,
            reason: None,
        },
    };
    store.apply_account_ban(&second_ban).await.unwrap();
    let first_page = store.list_active_account_bans(200, None, 1).await.unwrap();
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0].canonical_name, "beta");
    assert!(
        store
            .list_active_account_bans(200, Some("beta"), 1)
            .await
            .unwrap()
            .is_empty()
    );
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    sqlx::query("UPDATE accounts SET disabled=true WHERE id=$1")
        .bind(second as i64)
        .execute(&raw)
        .await
        .unwrap();
    assert_eq!(
        store.account_ban_verdict(second, 200).await.unwrap(),
        AccountBanVerdict::Disabled
    );
    assert!(store.list_active_account_bans(200, None, 0).await.is_err());
    let overlong = AccountBanOperation {
        operation_id: [17; 16],
        account_id: target,
        expected_revision: 5,
        issuer_account_id: None,
        change: AccountBanChange::Ban {
            started_unix_millis: 100,
            expires_unix_millis: 300,
            reason: Some("x".repeat(2049)),
        },
    };
    assert!(matches!(
        store.apply_account_ban(&overlong).await,
        Err(StoreError::Invalid(_))
    ));
    raw.close().await;
    store.close().await;
}
