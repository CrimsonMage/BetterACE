use super::*;
use bace_persistence::{CharacterLease, OwnershipState};
#[tokio::test]
async fn online_instance_is_durable_fenced_once_per_successful_login() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let offline = store
        .create_owned_character(&snapshot(9001, 0, b"counter-owner"))
        .await
        .unwrap();
    let loading = store.begin_login(offline).await.unwrap().lease;
    let (first, duplicate) = tokio::join!(
        store.finish_login_receipt(loading),
        store.finish_login_receipt(loading)
    );
    let receipt = match (first, duplicate) {
        (Ok(receipt), Err(StoreError::OwnershipConflict))
        | (Err(StoreError::OwnershipConflict), Ok(receipt)) => receipt,
        other => panic!("only one CAS may increment: {other:?}"),
    };
    assert_eq!(receipt.total_logins, 1);
    assert_eq!(
        store.online_login_receipt(receipt.lease).await.unwrap(),
        receipt
    );
    assert!(matches!(
        store.online_login_receipt(loading).await,
        Err(StoreError::OwnershipConflict)
    ));
    let stale = receipt.lease;
    let logging_out = store.begin_logout(receipt.lease).await.unwrap();
    let (offline, _) = store
        .finish_logout(logging_out, &snapshot(9001, 1, b"logged-out"))
        .await
        .unwrap();
    assert!(store.online_login_receipt(stale).await.is_err());
    let aborted = store.begin_login(offline).await.unwrap();
    let offline = store.abort_loading(aborted.lease).await.unwrap();
    let loading = store.begin_login(offline).await.unwrap().lease;
    let second = store.finish_login_receipt(loading).await.unwrap();
    assert_eq!(second.total_logins, 2, "aborted loading must not count");
    drop(store);
    let reopened = PgStore::connect(&cluster.url(), 2).await.unwrap();
    assert_eq!(
        reopened.online_login_receipt(second.lease).await.unwrap(),
        second
    );
    assert!(
        reopened
            .online_login_receipt(CharacterLease {
                epoch: second.lease.epoch + 1,
                ..second.lease
            })
            .await
            .is_err()
    );
}
#[tokio::test]
async fn counter_exhaustion_cannot_partially_transition_online() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let offline = store
        .create_owned_character(&snapshot(9002, 0, b"counter-limit"))
        .await
        .unwrap();
    let connection = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    sqlx::query("UPDATE character_ownership SET total_logins=2147483647 WHERE character_id=9002")
        .execute(&connection)
        .await
        .unwrap();
    let loading = store.begin_login(offline).await.unwrap().lease;
    assert!(matches!(
        store.finish_login_receipt(loading).await,
        Err(StoreError::Invalid("character login counter exhausted"))
    ));
    assert_eq!(store.character_lease(9002).await.unwrap(), Some(loading));
    assert_eq!(loading.state, OwnershipState::Loading);
    store.abort_loading(loading).await.unwrap();
}
