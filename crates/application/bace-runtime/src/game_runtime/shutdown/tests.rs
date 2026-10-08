use super::*;
#[tokio::test]
async fn admitted_owner_survives_rejected_shutdown_and_releases_only_after_idle_drain() {
    let (_cluster, _directory, runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let store = runtime.bootstrap.store.clone();
    let epoch = runtime.bootstrap.world_owner.epoch();
    let mut runtime = match runtime.into_shutdown() {
        Err((_, runtime)) => runtime,
        Ok(_) => panic!("live runtime cannot manufacture a drain proof"),
    };
    assert_eq!(runtime.bootstrap.world_owner.epoch(), epoch);
    assert!(store.acquire_world_owner().await.is_err());
    runtime.quiesce_now().unwrap();
    for _ in 0..1000 {
        runtime.poll_now().unwrap();
        if !runtime.requires_drain() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(!runtime.requires_drain(), "{:?}", runtime.status());
    let mut shutdown = runtime
        .into_shutdown()
        .unwrap_or_else(|(error, _)| panic!("{error}"));
    assert!(!shutdown.complete());
    assert!(store.acquire_world_owner().await.is_err());
    for _ in 0..1000 {
        if shutdown.poll().unwrap() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(shutdown.complete(), "{:?}", shutdown.failure());
    let new_owner = store.acquire_world_owner().await.unwrap();
    assert!(new_owner.epoch() > epoch);
    new_owner.close().await.unwrap();
}
