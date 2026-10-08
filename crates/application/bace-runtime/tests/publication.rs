use bace_content_tools::compile;
use bace_db_postgres::PgStore;
use bace_persistence::ContentCandidate;
use bace_runtime::{PublicationResult, load_catalog, publish_pending_once};
use std::process::Command;
struct Cluster(tempfile::TempDir);
impl Cluster {
    fn start() -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let output = Command::new("initdb")
            .args([
                "-A",
                "trust",
                "-U",
                "bace_test",
                "--no-locale",
                "-E",
                "UTF8",
                "-D",
            ])
            .arg(&data)
            .output()
            .expect("PostgreSQL initdb required for publication integration");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let options = format!("-h '' -k {}", root.path().display());
        let output = Command::new("pg_ctl")
            .arg("-D")
            .arg(data)
            .arg("-l")
            .arg(root.path().join("log"))
            .args(["-o", &options, "-w", "start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self(root)
    }
    fn url(&self) -> String {
        format!(
            "postgresql:///postgres?host={}&user=bace_test",
            self.0.path().display()
        )
    }
}
impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = Command::new("pg_ctl")
            .arg("-D")
            .arg(self.0.path().join("data"))
            .args(["-m", "immediate", "-w", "stop"])
            .output();
    }
}
fn candidate(id: u32) -> ContentCandidate {
    let name = format!("native_{id}");
    let bytes = compile(&format!(
        "schema_version=1\nweenie_id={id}\nclass_name=\"{name}\"\nweenie_type=1\n"
    ))
    .unwrap();
    ContentCandidate {
        wcid: id,
        class_name: name,
        weenie_type: 1,
        bytes,
    }
}
#[tokio::test]
async fn validates_off_thread_quarantines_then_delivers_whole_generation() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let mut catalog = load_catalog(&store).await.unwrap();
    let original = catalog.snapshot();
    let (delivery, mut receiver) = tokio::sync::mpsc::channel(1);
    let mut mismatched = candidate(1);
    mismatched.wcid = 2;
    let invalid = store.insert_candidates(&[mismatched]).await.unwrap();
    let valid = store.insert_candidates(&[candidate(3)]).await.unwrap();
    assert!(
        matches!(publish_pending_once(&store,&mut catalog,&delivery).await.unwrap(),PublicationResult::Rejected{revision,..} if revision==invalid)
    );
    assert!(store.active_content().await.unwrap().is_empty());
    assert!(receiver.try_recv().is_err());
    assert_eq!(
        publish_pending_once(&store, &mut catalog, &delivery)
            .await
            .unwrap(),
        PublicationResult::Accepted { revision: valid }
    );
    assert!(original.is_empty());
    let delivered = receiver.recv().await.unwrap();
    assert_eq!(delivered.revision(), valid as u64);
    assert_eq!(delivered.by_class_name("native_3").unwrap().weenie_id, 3);
    assert_eq!(
        load_catalog(&store).await.unwrap().snapshot().revision(),
        valid as u64
    );
    // A full channel blocks acceptance on the async worker, while existing readers retain Arc.
    delivery.send(delivered.clone()).await.unwrap();
    let next = store.insert_candidates(&[candidate(4)]).await.unwrap();
    let worker_store = store.clone();
    let mut worker =
        tokio::spawn(
            async move { publish_pending_once(&worker_store, &mut catalog, &delivery).await },
        );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut worker)
            .await
            .is_err()
    );
    assert_eq!(store.active_content().await.unwrap().len(), 1);
    receiver.recv().await.unwrap();
    assert_eq!(
        worker.await.unwrap().unwrap(),
        PublicationResult::Accepted { revision: next }
    );
    assert_eq!(receiver.recv().await.unwrap().len(), 2);
    assert_eq!(delivered.len(), 1);
    store.close().await;
}

#[tokio::test]
async fn reserved_postgres_save_worker_commits_and_drains() {
    use bace_persistence::SaveSnapshot;
    use bace_runtime::saves::{SaveWorkerConfig, WriteOutcome, spawn_postgres_save_worker};
    let cluster = Cluster::start();
    let admin = PgStore::connect(&cluster.url(), 1).await.unwrap();
    admin.migrate().await.unwrap();
    let worker = spawn_postgres_save_worker(&cluster.url(), SaveWorkerConfig::default())
        .await
        .unwrap();
    let first = SaveSnapshot {
        object_id: 71,
        mutation_revision: 1,
        expected_version: 0,
        bytes: b"routine".to_vec(),
    };
    let ticket = worker
        .handle
        .try_routine(
            &[first],
            tokio::time::Instant::now() - std::time::Duration::from_secs(5),
        )
        .unwrap();
    assert!(matches!(
        ticket.await.unwrap().result,
        Ok(WriteOutcome::Routine(_))
    ));
    let changed = SaveSnapshot {
        object_id: 71,
        mutation_revision: 2,
        expected_version: 1,
        bytes: b"valuable".to_vec(),
    };
    let valuable = worker
        .handle
        .try_valuable("worker-operation", &[changed])
        .unwrap();
    assert!(matches!(
        valuable.await.unwrap().result,
        Ok(WriteOutcome::Valuable(_))
    ));
    assert_eq!(admin.load(71).await.unwrap().unwrap().bytes, b"valuable");
    drop(worker.handle);
    let summary = worker.task.await.unwrap();
    assert_eq!(summary.routine_completed, 1);
    assert_eq!(summary.valuable_completed, 1);
    assert_eq!(summary.failed, 0);
    assert!(!summary.shutdown_timed_out);
    admin.close().await;
}

#[path = "publication/native.rs"]
mod native;

#[path = "publication/mixed.rs"]
mod mixed;

#[path = "publication/mapped.rs"]
mod mapped;

#[path = "publication/death.rs"]
mod death;
