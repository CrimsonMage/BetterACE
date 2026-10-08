use super::*;
#[tokio::test]
async fn cancelled_drain_keeps_the_same_operation_and_requires_a_real_owner_receipt() {
    let (controls, mut requests) = mpsc::channel(2);
    let (publish, status) = watch::channel(Status {
        ready: true,
        drained: false,
        detail: "fixture".into(),
    });
    let lost = Arc::new(AtomicBool::new(false));
    let mut backend = GameBackend {
        controls,
        status,
        lost: lost.clone(),
        operation: None,
    };
    assert!(
        tokio::time::timeout(Duration::from_millis(10), backend.drain(41))
            .await
            .is_err()
    );
    assert_eq!(requests.try_recv().unwrap(), 41);
    assert!(
        backend.drain(42).await.is_err(),
        "another operation cannot replace the retained drain"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), backend.drain(41))
            .await
            .is_err()
    );
    assert!(
        requests.try_recv().is_err(),
        "retry must not create another drain owner"
    );
    publish.send_replace(Status {
        ready: false,
        drained: true,
        detail: "proof".into(),
    });
    backend.drain(41).await.unwrap();
    drop(backend);
    assert!(lost.load(Ordering::Acquire));
}
#[tokio::test]
async fn worker_channel_loss_is_not_a_durable_shutdown_receipt() {
    let (controls, _requests) = mpsc::channel(2);
    let (publish, status) = watch::channel(Status {
        ready: false,
        drained: false,
        detail: "retained".into(),
    });
    let mut backend = GameBackend {
        controls,
        status,
        lost: Arc::new(AtomicBool::new(false)),
        operation: None,
    };
    drop(publish);
    assert!(backend.drain(1).await.is_err());
}
