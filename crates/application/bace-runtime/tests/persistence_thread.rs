use bace_db_postgres::PgStore;
use std::{process::Command, time::Duration};
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
#[tokio::test]
async fn dedicated_postgres_thread_commits_owned_saves_and_offline_receipts() {
    use bace_persistence::{OfflineXpEvent, SaveSnapshot};
    use bace_runtime::{
        persistence_thread::PersistenceThread,
        saves::{SaveWorkerConfig, WriteOutcome},
    };
    let cluster = Cluster::start();
    let admin = PgStore::connect(&cluster.url(), 2).await.unwrap();
    admin.migrate().await.unwrap();
    let mut snapshot = SaveSnapshot {
        object_id: 31,
        mutation_revision: 1,
        expected_version: 0,
        bytes: b"character".to_vec(),
    };
    let offline = admin.create_owned_character(&snapshot).await.unwrap();
    let worker = PersistenceThread::postgres(cluster.url(), SaveWorkerConfig::default())
        .await
        .unwrap();
    assert_ne!(worker.thread_id, std::thread::current().id());
    let event = OfflineXpEvent {
        event_id: "thread-xp".into(),
        source_character: 12,
        target_character: 31,
        amount: 700,
    };
    let report = worker
        .handle
        .try_offline(
            &event,
            offline,
            tokio::time::Instant::now() - Duration::from_secs(5),
        )
        .unwrap()
        .await
        .unwrap();
    assert!(
        matches!(report.result,Ok(WriteOutcome::Offline(r)) if r.cached_xp==700 && r.newly_applied)
    );
    let loaded = admin.begin_login(offline).await.unwrap();
    assert_eq!(loaded.cached_xp, 700);
    let online = admin.finish_login(loaded.lease).await.unwrap();
    snapshot.expected_version = 1;
    snapshot.mutation_revision = 2;
    snapshot.bytes = b"saved online".to_vec();
    assert!(
        worker
            .handle
            .try_owned_routine(online, &snapshot, tokio::time::Instant::now())
            .unwrap()
            .await
            .unwrap()
            .result
            .is_ok()
    );
    let report = tokio::time::timeout(Duration::from_secs(3), worker.drain())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.offline_completed, 1);
    assert_eq!(report.routine_completed, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(
        admin.load(31).await.unwrap().unwrap().bytes,
        b"saved online"
    );
    admin.close().await;
}
