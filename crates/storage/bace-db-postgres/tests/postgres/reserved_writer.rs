use super::*;
use std::time::Duration;
#[tokio::test]
async fn writer_preserves_admitted_database_and_enforces_independent_deadlines() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    store.bind_random_key(1, [7; 32]).await.unwrap();
    assert!(store.reserved_writer(Duration::ZERO).await.is_err());
    let writer = store
        .reserved_writer(Duration::from_millis(200))
        .await
        .unwrap();
    store.close().await;
    // Closing the general-read pool does not close the reserved writer, and its
    // identity still sees the fingerprint from the admitted database.
    writer.bind_random_key(1, [7; 32]).await.unwrap();
    assert!(matches!(
        writer.bind_random_key(1, [8; 32]).await,
        Err(StoreError::Invalid(_))
    ));
    let other = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let mut lock = other.begin().await.unwrap();
    sqlx::query("INSERT INTO random_key_fingerprints(version,sha256) VALUES(2,$1)")
        .bind([9u8; 32].as_slice())
        .execute(&mut *lock)
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), writer.bind_random_key(2, [9; 32]))
        .await
        .unwrap();
    let Err(StoreError::Sql(sqlx::Error::Database(error))) = result else {
        panic!("server deadline must stop blocked writer")
    };
    assert_eq!(error.code().as_deref(), Some("55P03"));
    lock.rollback().await.unwrap();
    writer.bind_random_key(2, [9; 32]).await.unwrap();
    writer.close().await;
}
