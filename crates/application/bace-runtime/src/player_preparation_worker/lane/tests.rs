use super::*;
#[test]
fn cold_input_and_unclaimed_output_share_capacity_and_shutdown_retains_owner() {
    let (release, wait) = mpsc::channel();
    let (done, completed) = mpsc::channel();
    let mut lane = Lane::start(1, move |job: std::sync::Arc<u32>| {
        wait.recv().unwrap();
        done.send(()).unwrap();
        (job, Err::<(), _>("missing required DAT asset"))
    })
    .unwrap();
    let first = std::sync::Arc::new(11);
    let second = std::sync::Arc::new(12);
    lane.try_submit(1, first.clone()).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &lane.try_submit(2, second.clone()).unwrap_err(),
        &second
    ));
    let mut lane = *lane
        .try_shutdown()
        .expect_err("pending work must retain worker");
    release.send(()).unwrap();
    completed
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        lane.pending(),
        1,
        "completed unread work still occupies admission"
    );
    assert!(lane.try_submit(2, second.clone()).is_err());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let (returned, result) = loop {
        if let Some(value) = lane.try_recv().unwrap() {
            break value;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    };
    assert!(std::sync::Arc::ptr_eq(&returned, &first));
    assert_eq!(result, Err("missing required DAT asset"));
    assert_eq!(lane.pending(), 0);
    lane.try_shutdown().ok().unwrap().join().unwrap();
}
#[test]
fn duplicate_or_zero_correlation_never_consumes_an_input() {
    let (release, wait) = mpsc::channel();
    let mut lane = Lane::start(2, move |job: u32| {
        wait.recv().unwrap();
        job
    })
    .unwrap();
    assert_eq!(lane.try_submit(0, 7), Err(7));
    lane.try_submit(9, 8).unwrap();
    assert_eq!(lane.try_submit(9, 10), Err(10));
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(value) = lane.try_recv().unwrap() {
            assert_eq!(value, 8);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
    lane.try_shutdown().ok().unwrap().join().unwrap();
}
