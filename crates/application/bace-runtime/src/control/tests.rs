use super::*;
use crate::supervisor_child::{HostBackend, run_child};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::net::TcpListener;

#[tokio::test]
async fn mutual_auth_rejects_wrong_capability_and_bounds_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let metadata = ChildMetadata {
        version: 1,
        port: listener.local_addr().unwrap().port(),
        generation: 7,
        pid: 0,
        capability: random_bytes().unwrap(),
    };
    let expected = metadata.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_auth(&mut stream, &expected).await
    });
    let mut wrong = metadata.clone();
    wrong.capability[0] ^= 1;
    assert!(connect(&wrong).await.is_err());
    assert!(server.await.unwrap().is_err());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let sender = tokio::spawn(async move {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stream.write_u32_le((FRAME_LIMIT + 1) as u32).await.unwrap();
    });
    let (mut stream, _) = listener.accept().await.unwrap();
    assert!(read_frame::<ChildRequest>(&mut stream).await.is_err());
    sender.await.unwrap();
}

struct RetainedBackend {
    attempts: Arc<AtomicUsize>,
}
impl HostBackend for RetainedBackend {
    fn status(&self) -> (bool, String) {
        (false, "Test backend holds dirty state.".into())
    }
    fn control_lost(&mut self) {}
    async fn drain(&mut self, _: u64) -> Result<(), String> {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err("database unavailable".into())
        } else {
            Ok(())
        }
    }
}
#[cfg(not(windows))]
#[tokio::test]
async fn child_preserves_blocked_operation_across_reconnect_and_refuses_early_exit() {
    let directory = tempfile::tempdir().unwrap();
    let state = directory.path().join("private");
    let attempts = Arc::new(AtomicUsize::new(0));
    let backend = RetainedBackend {
        attempts: attempts.clone(),
    };
    let child_path = state.clone();
    let child =
        tokio::spawn(
            async move { run_child(&child_path, 99, Duration::from_secs(1), backend).await },
        );
    let metadata = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(metadata) = load_metadata(&state.join("child.json")) {
                break metadata;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    async fn request(metadata: &ChildMetadata, request: ChildRequest) -> ChildReply {
        let mut stream = connect(metadata).await.unwrap();
        write_frame(&mut stream, &request).await.unwrap();
        read_frame(&mut stream).await.unwrap()
    }
    assert!(matches!(
        request(&metadata, ChildRequest::Exit { operation_id: 42 }).await,
        ChildReply::Blocked { .. }
    ));
    assert!(matches!(
        request(&metadata, ChildRequest::Drain { operation_id: 42 }).await,
        ChildReply::Blocked { .. }
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(lock(&state.join("child.lock")).is_err());
    assert!(matches!(
        request(&metadata, ChildRequest::Status).await,
        ChildReply::Status {
            operation_id: Some(42),
            ..
        }
    ));
    assert!(matches!(
        request(&metadata, ChildRequest::Drain { operation_id: 43 }).await,
        ChildReply::Blocked { .. }
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(matches!(
        request(&metadata, ChildRequest::Drain { operation_id: 42 }).await,
        ChildReply::Drained { .. }
    ));
    assert!(matches!(
        request(&metadata, ChildRequest::Exit { operation_id: 42 }).await,
        ChildReply::Exiting { .. }
    ));
    tokio::time::timeout(Duration::from_secs(3), child)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(lock(&state.join("child.lock")).is_ok());
}
