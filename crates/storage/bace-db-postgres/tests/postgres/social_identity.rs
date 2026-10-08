use super::*;
use bace_db_postgres::PlayerIdentityQuery;
#[tokio::test]
async fn offline_social_names_and_batched_ids_are_indexed_and_bounded() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let account:i64=sqlx::query_scalar("INSERT INTO accounts(canonical_name,password_phc) VALUES('social','$fixture$') RETURNING id").fetch_one(&raw).await.unwrap();
    for (slot, name) in ["Élodie", "Rune"].into_iter().enumerate() {
        let id = 0x50000001i64 + slot as i64;
        sqlx::query("INSERT INTO entity_snapshots(object_id,version,payload) VALUES($1,1,$2)")
            .bind(id)
            .bind(vec![0u8])
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO character_ownership(character_id) VALUES($1)")
            .bind(id)
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO players(object_id,account_id,name,canonical_name,slot) VALUES($1,$2,$3,$4,$5)").bind(id).bind(account).bind(name).bind(name.to_lowercase()).bind(slot as i16).execute(&raw).await.unwrap();
    }
    let found = store
        .lookup_player_identity(&PlayerIdentityQuery::Name("ÉLODIE".into()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .lookup_player_identity(&PlayerIdentityQuery::Name("++ÉLODIE".into()))
            .await
            .unwrap(),
        Some(found.clone())
    );
    assert_eq!(found.name, "Élodie");
    assert_eq!(found.object_id, 0x50000001);
    assert_eq!(found.account_id, account as u64);
    assert_eq!(
        store
            .lookup_player_identity(&PlayerIdentityQuery::Character(found.object_id))
            .await
            .unwrap(),
        Some(found)
    );
    assert!(
        store
            .lookup_player_identity(&PlayerIdentityQuery::Name("unknown".into()))
            .await
            .unwrap()
            .is_none()
    );
    let batch = store
        .lookup_player_identities(&[0x50000002, 0x50000001, 0x50000003])
        .await
        .unwrap();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].name, "Élodie");
    assert_eq!(batch[1].name, "Rune");
    assert!(
        store
            .lookup_player_identities(&[0x50000001, 0x50000001])
            .await
            .is_err()
    );
    assert!(
        store
            .lookup_player_identities(&vec![0x50000001; 1025])
            .await
            .is_err()
    );
    assert!(store.lookup_player_identities(&[0]).await.is_err());
    assert!(
        store
            .lookup_player_identity(&PlayerIdentityQuery::Name("x".repeat(101)))
            .await
            .is_err()
    );
    raw.close().await;
    store.close().await;
}
