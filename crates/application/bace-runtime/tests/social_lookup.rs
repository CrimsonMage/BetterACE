use std::process::Command;
struct Cluster {
    directory: tempfile::TempDir,
}
impl Cluster {
    fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
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
            .expect("PostgreSQL initdb must be installed for database tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let options = format!("-h '' -k {}", directory.path().display());
        let output = Command::new("pg_ctl")
            .arg("-D")
            .arg(&data)
            .arg("-l")
            .arg(directory.path().join("log"))
            .args(["-o", &options, "-w", "start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self { directory }
    }
    fn url(&self) -> String {
        format!(
            "postgresql:///postgres?host={}&user=bace_test",
            self.directory.path().display()
        )
    }
}
impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = Command::new("pg_ctl")
            .arg("-D")
            .arg(self.directory.path().join("data"))
            .args(["-m", "immediate", "-w", "stop"])
            .output();
    }
}
use bace_db_postgres::PgStore;
use bace_gameplay_api::{ActionContext, SessionId, social::SocialRequest};
use bace_runtime::social_lookup::{PendingSocialAction, SocialLookupService};
use bace_types::{AccountId, EntityId};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn action(sequence: u32) -> PendingSocialAction {
    PendingSocialAction {
        context: ActionContext {
            actor: EntityId(0x50000001),
            account: AccountId(1),
            session: SessionId(7),
            sequence,
        },
        request: SocialRequest::AddFriend("Missing".into()),
    }
}
#[tokio::test]
async fn identity_reads_yield_to_save_pressure_and_shutdown_recovers_unadmitted_action() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 1).await.unwrap();
    store.migrate().await.unwrap();
    let observer = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let pressure = Arc::new(AtomicBool::new(true));
    let mut lookup =
        SocialLookupService::start_with_pressure(store.clone(), 2, pressure.clone()).unwrap();
    lookup.begin(action(3)).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE query LIKE 'SELECT object_id,account_id,name FROM players WHERE canonical_name=%'").fetch_one(&observer).await.unwrap();
    assert_eq!(
        count, 0,
        "no identity read may start while routine saves have deadline debt"
    );
    assert_eq!(lookup.poll(2).unwrap(), 0);
    pressure.store(false, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if lookup.poll(2).unwrap() > 0 {
            break;
        }
        assert!(Instant::now() < deadline, "identity read did not resume");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut received = false;
    assert_eq!(lookup.flush(2,|command|{
        assert!(matches!(command,bace_simulation::Command::SocialResolved{context,identity:None,..} if context.sequence==3));received=true;Ok(())
    }),1);
    assert!(received);
    pressure.store(true, Ordering::Release);
    lookup.begin(action(4)).unwrap();
    let started = Instant::now();
    let (pending, result) = tokio::task::spawn_blocking(move || lookup.stop())
        .await
        .unwrap();
    result.unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].context.sequence, 4);
    observer.close().await;
    store.close().await;
}
