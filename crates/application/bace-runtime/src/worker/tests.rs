use super::*;
use bace_content_tools::compile;
use bace_persistence::ContentCandidate;
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

async fn wait_for_lock(pool: &sqlx::PgPool, query: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE $1)")
                .bind(query).fetch_one(pool).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("worker must reach blocked PostgreSQL query");
}

#[tokio::test]
async fn shutdown_cancels_startup_and_publication_then_restart_recovers_heads() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let initial = store.insert_candidates(&[candidate(1)]).await.unwrap();
    store.accept_validated(initial).await.unwrap();
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    // First block startup's accepted-head read, then block an actual acceptance transaction.
    for startup in [true, false] {
        let mut lock = raw.begin().await.unwrap();
        if startup {
            sqlx::query("LOCK TABLE content_heads IN ACCESS EXCLUSIVE MODE")
                .execute(&mut *lock)
                .await
                .unwrap();
        } else {
            store.insert_candidates(&[candidate(2)]).await.unwrap();
            sqlx::query("SELECT pg_advisory_xact_lock(42812001)")
                .execute(&mut *lock)
                .await
                .unwrap();
        }
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let url = cluster.url();
        let worker = tokio::spawn(async move {
            run_until(
                &url,
                2,
                Duration::from_secs(10),
                Duration::from_millis(250),
                async {
                    stopped.await.unwrap();
                    Ok(())
                },
            )
            .await
        });
        wait_for_lock(
            &raw,
            if startup {
                "SELECT c.wcid%"
            } else {
                "SELECT pg_advisory_xact_lock(42812001)%"
            },
        )
        .await;
        stop.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .expect("shutdown must not wait for the externally held lock")
            .unwrap();
        assert!(matches!(
            result,
            Ok(()) | Err(WorkerError::Deadline("pool shutdown"))
        ));
        lock.rollback().await.unwrap();
        assert_eq!(
            load_catalog(&store).await.unwrap().snapshot().revision(),
            initial as u64
        );
    }
    // Canceled acceptance leaves durable pending work recoverable by a new worker.
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let url = cluster.url();
    let worker = tokio::spawn(async move {
        run_until(
            &url,
            2,
            Duration::from_secs(10),
            Duration::from_secs(1),
            async {
                stopped.await.unwrap();
                Ok(())
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while store.content_status().await.unwrap().active_templates != 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    worker.await.unwrap().unwrap();
    assert_eq!(load_catalog(&store).await.unwrap().snapshot().len(), 2);
    raw.close().await;
    store.close().await;
}
