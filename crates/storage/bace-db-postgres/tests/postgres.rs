//! Real PostgreSQL test. Requires initdb and pg_ctl on PATH; never touches an existing cluster.
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{ContentCandidate, OperationOutcome, SaveSnapshot};
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
fn candidate(wcid: u32, bytes: &[u8]) -> ContentCandidate {
    ContentCandidate {
        wcid,
        class_name: format!("weenie_{wcid}"),
        weenie_type: 1,
        bytes: bytes.to_vec(),
    }
}
fn snapshot(id: u32, expected_version: i64, bytes: &[u8]) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        mutation_revision: 55,
        expected_version,
        bytes: bytes.to_vec(),
    }
}
#[tokio::test]
async fn actual_database_publication_cas_and_idempotency() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    store.migrate().await.unwrap();
    let initial = store
        .insert_candidates(&[candidate(1, b"good")])
        .await
        .unwrap();
    assert!(store.active_content().await.unwrap().is_empty());
    store.accept_validated(initial).await.unwrap();
    let invalid = store
        .insert_candidates(&[candidate(1, b"invalid")])
        .await
        .unwrap();
    let later = store
        .insert_candidates(&[candidate(2, b"also good")])
        .await
        .unwrap();
    assert!(matches!(
        store.accept_validated(later).await,
        Err(StoreError::PublicationOrder)
    ));
    store
        .reject(invalid, "semantic validation failed")
        .await
        .unwrap();
    store.accept_validated(later).await.unwrap();
    assert_eq!(
        store.active_content().await.unwrap(),
        vec![candidate(1, b"good"), candidate(2, b"also good")]
    );
    assert!(store.pending_publications(0, 100).await.unwrap().is_empty());
    // A fresh connection loads accepted heads rather than the latest invalid candidate.
    let restarted = PgStore::connect(&cluster.url(), 2).await.unwrap();
    assert_eq!(restarted.active_content().await.unwrap()[0].bytes, b"good");
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let raw_revision:i64=sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES(3,'native_sql',1,$1) RETURNING revision")
        .bind(b"raw".as_slice()).fetch_one(&raw).await.unwrap();
    assert_eq!(
        store.pending_publications(later, 10).await.unwrap()[0].revision,
        raw_revision
    );
    assert!(
        sqlx::query("UPDATE content_candidates SET payload=$1 WHERE wcid=3")
            .bind(b"changed".as_slice())
            .execute(&raw)
            .await
            .is_err()
    );
    store.accept_validated(raw_revision).await.unwrap();
    let base = vec![snapshot(10, 0, b"owner A"), snapshot(11, 0, b"owner B")];
    store.save_batch(&base).await.unwrap();
    let trade = vec![snapshot(10, 1, b"owner B"), snapshot(11, 1, b"owner A")];
    assert!(matches!(
        store.valuable("trade-1", &trade).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.valuable("trade-1", &trade).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    assert!(store.resolve_operation("trade-1").await.unwrap().is_some());
    assert!(matches!(
        store
            .valuable("trade-1", &[snapshot(10, 2, b"different")])
            .await,
        Err(StoreError::OperationMismatch)
    ));
    assert!(matches!(
        store.save_batch(&[snapshot(10, 1, b"old snapshot")]).await,
        Err(StoreError::Conflict(10))
    ));
    assert_eq!(store.load(10).await.unwrap().unwrap().bytes, b"owner B");
    // Object 10 would succeed first; object 11 conflicts. The transaction rolls both back.
    assert!(
        store
            .valuable(
                "trade-fails",
                &[snapshot(10, 2, b"must rollback"), snapshot(11, 1, b"stale")]
            )
            .await
            .is_err()
    );
    assert_eq!(store.load(10).await.unwrap().unwrap().bytes, b"owner B");
    assert!(
        store
            .resolve_operation("trade-fails")
            .await
            .unwrap()
            .is_none()
    );
    // Allocating candidate revisions is serialized through commit, including raw SQL.
    let mut first = raw.begin().await.unwrap();
    let rev1:i64=sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES(4,'four',1,$1) RETURNING revision")
        .bind(b"first".as_slice()).fetch_one(&mut *first).await.unwrap();
    let second_store = store.clone();
    let mut second = tokio::spawn(async move {
        second_store
            .insert_candidates(&[candidate(5, b"second")])
            .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut second)
            .await
            .is_err()
    );
    first.commit().await.unwrap();
    assert_eq!(second.await.unwrap().unwrap(), rev1 + 1);
    let mut rolled_back = raw.begin().await.unwrap();
    let rolled_revision: i64 = sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES(6,'rolled_back',1,$1) RETURNING revision")
        .bind(b"invisible".as_slice()).fetch_one(&mut *rolled_back).await.unwrap();
    rolled_back.rollback().await.unwrap();
    let grouped = store
        .insert_candidates(&[candidate(6, b"six"), candidate(7, b"seven")])
        .await
        .unwrap();
    assert_eq!(
        grouped, rolled_revision,
        "transactional counter rollback must not publish a gap"
    );
    let pending = store.pending_publications(grouped - 1, 1).await.unwrap();
    assert_eq!(
        pending[0].candidates.len(),
        2,
        "one transaction must remain one atomic publication"
    );
    assert_eq!(pending[0].candidates[0].bytes, b"six");
    raw.close().await;
    restarted.close().await;
    store.close().await;
}

#[tokio::test]
async fn fresh_accounts_are_unique_unprivileged_and_preserve_credentials() {
    use bace_auth::{
        AccessLevel, AccountName, AccountRepository, CreateAccountOutcome, NewAccount,
        PasswordService,
    };
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let password_hash = tokio::task::spawn_blocking(|| {
        PasswordService::new(1)
            .unwrap()
            .hash(b"correct test password")
            .unwrap()
    })
    .await
    .unwrap();
    let new = NewAccount {
        name: AccountName::parse("FreshPlayer").unwrap(),
        password_hash: password_hash.clone(),
    };
    let (first, second) = tokio::join!(store.create(new.clone()), store.create(new));
    let outcomes = [first.unwrap(), second.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, CreateAccountOutcome::Created(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, CreateAccountOutcome::AlreadyExists))
            .count(),
        1
    );
    let account = store
        .find_by_name(&AccountName::parse("FRESHPLAYER").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(account.id.0 > 0);
    assert_eq!(account.name.as_str(), "freshplayer");
    assert_eq!(
        account.access_level,
        AccessLevel::Player,
        "first account must never auto-promote"
    );
    assert!(!account.disabled);
    assert_eq!(account.password_hash, password_hash);
    assert!(
        store
            .find_by_name(&AccountName::parse("missing").unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let different = tokio::task::spawn_blocking(|| {
        PasswordService::new(1)
            .unwrap()
            .hash(b"different password")
            .unwrap()
    })
    .await
    .unwrap();
    assert!(matches!(
        store
            .create(NewAccount {
                name: account.name.clone(),
                password_hash: different
            })
            .await
            .unwrap(),
        CreateAccountOutcome::AlreadyExists
    ));
    let unchanged = store.find_by_name(&account.name).await.unwrap().unwrap();
    assert_eq!(unchanged.password_hash, password_hash);
    let verified = tokio::task::spawn_blocking(move || {
        PasswordService::new(1)
            .unwrap()
            .verify(b"correct test password", &unchanged.password_hash)
            .unwrap()
    })
    .await
    .unwrap();
    assert!(verified);
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    sqlx::query("UPDATE accounts SET access_level=5,disabled=true WHERE id=$1")
        .bind(account.id.0 as i64)
        .execute(&raw)
        .await
        .unwrap();
    let updated = store.find_by_name(&account.name).await.unwrap().unwrap();
    assert_eq!(updated.access_level, AccessLevel::Admin);
    assert!(updated.disabled);
    assert!(
        sqlx::query("UPDATE accounts SET access_level=6")
            .execute(&raw)
            .await
            .is_err()
    );
    sqlx::query("UPDATE accounts SET password_phc='invalid PHC'")
        .execute(&raw)
        .await
        .unwrap();
    assert!(matches!(
        store.find_by_name(&account.name).await,
        Err(StoreError::Invalid(_))
    ));
    raw.close().await;
    store.close().await;
}

#[tokio::test]
async fn writer_server_lock_deadline_releases_capacity_for_unrelated_save() {
    let cluster = Cluster::start();
    let admin = PgStore::connect(&cluster.url(), 2).await.unwrap();
    admin.migrate().await.unwrap();
    admin
        .save_batch(&[snapshot(10, 0, b"initial")])
        .await
        .unwrap();
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let mut lock = raw.begin().await.unwrap();
    sqlx::query("SELECT object_id FROM entity_snapshots WHERE object_id=10 FOR UPDATE")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    let writer = PgStore::connect_writer(&cluster.url(), std::time::Duration::from_millis(200))
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let blocked = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        writer.save_batch(&[snapshot(10, 1, b"blocked")]),
    )
    .await
    .unwrap();
    assert!(matches!(
        blocked,
        Err(StoreError::Sql(sqlx::Error::Database(_)))
    ));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    // The external lock remains held. A different object still gets the same writer connection.
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        writer.save_batch(&[snapshot(11, 0, b"unrelated")]),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(admin.load(11).await.unwrap().unwrap().bytes, b"unrelated");
    assert_eq!(admin.load(10).await.unwrap().unwrap().bytes, b"initial");
    lock.rollback().await.unwrap();
    writer.close().await;
    raw.close().await;
    admin.close().await;
}

#[tokio::test]
async fn content_status_is_exact_and_requires_no_payload_access() {
    use bace_db_postgres::ContentStatus;
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    assert_eq!(
        store.content_status().await.unwrap(),
        ContentStatus::default()
    );
    let first = store
        .insert_candidates(&[candidate(1, b"first")])
        .await
        .unwrap();
    store.accept_validated(first).await.unwrap();
    let replacement = store
        .insert_candidates(&[candidate(1, b"replacement")])
        .await
        .unwrap();
    store.accept_validated(replacement).await.unwrap();
    let bad = store
        .insert_candidates(&[candidate(2, b"bad")])
        .await
        .unwrap();
    store.reject(bad, "invalid payload").await.unwrap();
    for id in 3..108 {
        store
            .insert_candidates(&[candidate(id, b"pending")])
            .await
            .unwrap();
    }
    let raw = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    sqlx::query("CREATE ROLE status_reader LOGIN")
        .execute(&raw)
        .await
        .unwrap();
    sqlx::query("GRANT SELECT ON content_heads,content_publications TO status_reader")
        .execute(&raw)
        .await
        .unwrap();
    let reader = PgStore::connect(
        &cluster
            .url()
            .replace("user=bace_test", "user=status_reader"),
        1,
    )
    .await
    .unwrap();
    assert_eq!(
        reader.content_status().await.unwrap(),
        ContentStatus {
            accepted_revision: replacement,
            active_templates: 1,
            pending_publications: 105,
            rejected_publications: 1,
        }
    );
    // Permission denial proves that the status API cannot read candidate payloads.
    assert!(reader.active_content().await.is_err());
    reader.close().await;
    raw.close().await;
    store.close().await;
}

#[tokio::test]
async fn mapped_generation_acceptance_is_atomic_parent_checked_and_retryable() {
    use bace_persistence::MappedGeneration;
    use sha2::{Digest, Sha256};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let build = |data: &[u8], parent_hash, accepted_revision| MappedGeneration {
        manifest_hash: Sha256::digest(data).into(),
        parent_hash,
        base_hash: [42; 32],
        accepted_revision,
        manifest_bytes: data.to_vec(),
    };
    let base = build(b"base", None, 0);
    store.accept_mapped(None, &base).await.unwrap();
    assert_eq!(store.active_generation().await.unwrap(), Some(base.clone()));
    let first = store
        .insert_candidates(&[candidate(21, b"mapped record")])
        .await
        .unwrap();
    let second = store
        .insert_candidates(&[candidate(22, b"next record")])
        .await
        .unwrap();
    assert!(matches!(
        store.accept_validated(first).await,
        Err(StoreError::Invalid(_))
    ));
    let out_of_order = build(b"wrong order", Some(base.manifest_hash), second);
    assert!(matches!(
        store.accept_mapped(Some(second), &out_of_order).await,
        Err(StoreError::PublicationOrder)
    ));
    assert!(store.active_content().await.unwrap().is_empty());
    let accepted = build(b"accepted", Some(base.manifest_hash), first);
    store.accept_mapped(Some(first), &accepted).await.unwrap();
    store.accept_mapped(Some(first), &accepted).await.unwrap();
    assert_eq!(store.active_content().await.unwrap().len(), 1);
    assert_eq!(store.pending_revision().await.unwrap(), Some(second));
    assert_eq!(
        store.candidate_page(second, 0, 1).await.unwrap()[0].wcid,
        22
    );
    assert!(
        store
            .candidate_page(second, 22, 1)
            .await
            .unwrap()
            .is_empty()
    );
    let stale = build(b"stale", Some(base.manifest_hash), second);
    assert!(matches!(
        store.accept_mapped(Some(second), &stale).await,
        Err(StoreError::GenerationConflict)
    ));
    let mut mismatched = accepted.clone();
    mismatched.base_hash = [11; 32];
    assert!(matches!(
        store.accept_mapped(Some(first), &mismatched).await,
        Err(StoreError::OperationMismatch)
    ));
    let compacted = build(b"compacted", Some(accepted.manifest_hash), first);
    store.accept_mapped(None, &compacted).await.unwrap();
    assert_eq!(store.active_generation().await.unwrap(), Some(compacted));
    assert_eq!(
        store.content_status().await.unwrap().accepted_revision,
        first
    );
    store.close().await;
}

#[tokio::test]
async fn offline_xp_receipts_and_login_logout_fences_prevent_stale_writes() {
    use bace_persistence::{OfflineXpEvent, OwnershipState};
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let offline = store
        .create_owned_character(&snapshot(100, 0, b"character"))
        .await
        .unwrap();
    let event = OfflineXpEvent {
        event_id: "xp-one".into(),
        source_character: 101,
        target_character: 100,
        amount: 150,
    };
    store.enqueue_xp_event(&event).await.unwrap();
    store.enqueue_xp_event(&event).await.unwrap();
    let mut changed = event.clone();
    changed.amount = 151;
    assert!(matches!(
        store.enqueue_xp_event(&changed).await,
        Err(StoreError::OperationMismatch)
    ));
    let receipt = store
        .apply_offline_xp(&event.event_id, offline)
        .await
        .unwrap();
    assert!(receipt.newly_applied);
    assert_eq!(receipt.cached_xp, 150);
    assert!(
        !store
            .apply_offline_xp(&event.event_id, offline)
            .await
            .unwrap()
            .newly_applied
    );
    let load = store.begin_login(offline).await.unwrap();
    assert_eq!(load.cached_xp, 150);
    assert_eq!(load.snapshot.persisted_version, 1);
    assert!(matches!(
        store.apply_offline_xp(&event.event_id, offline).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(matches!(
        store.save_batch(&[snapshot(100, 1, b"unfenced")]).await,
        Err(StoreError::OwnershipConflict)
    ));
    let online = store.finish_login(load.lease).await.unwrap();
    store
        .save_owned(online, &snapshot(100, 1, b"online save"))
        .await
        .unwrap();
    let exiting = store.begin_logout(online).await.unwrap();
    assert_eq!(exiting.state, OwnershipState::LoggingOut);
    assert!(matches!(
        store
            .save_owned(online, &snapshot(100, 2, b"late save"))
            .await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(matches!(
        store
            .finish_logout(exiting, &snapshot(100, 1, b"bad CAS"))
            .await,
        Err(StoreError::Conflict(100))
    ));
    assert_eq!(store.character_lease(100).await.unwrap(), Some(exiting));
    let (offline2, _) = store
        .finish_logout(exiting, &snapshot(100, 2, b"logout skills"))
        .await
        .unwrap();
    // Race an offline update against login. One locks the ownership first; never both owners.
    let race = OfflineXpEvent {
        event_id: "xp-race".into(),
        amount: 25,
        ..event
    };
    store.enqueue_xp_event(&race).await.unwrap();
    let (a, b) = tokio::join!(
        store.apply_offline_xp(&race.event_id, offline2),
        store.begin_login(offline2)
    );
    let loaded = b.unwrap();
    match a {
        Ok(r) => {
            assert_eq!(r.cached_xp, 175);
            assert_eq!(loaded.cached_xp, 175);
        }
        Err(StoreError::OwnershipConflict) => {
            assert_eq!(loaded.cached_xp, 150);
            assert_eq!(store.pending_xp_events(100, 10).await.unwrap().len(), 1);
        }
        other => panic!("unexpected race result {other:?}"),
    }
    assert_eq!(loaded.snapshot.bytes, b"logout skills");
    let offline3 = store.abort_loading(loaded.lease).await.unwrap();
    let retried = store
        .apply_offline_xp(&race.event_id, offline3)
        .await
        .unwrap();
    assert_eq!(retried.cached_xp, 175);
    assert!(store.pending_xp_events(100, 10).await.unwrap().is_empty());
    store.close().await;
}
