use super::{Cluster, candidate};
use bace_db_postgres::PgStore;
use bace_persistence::{MappedGeneration, NativeContentCandidate};
use bace_runtime::{
    PublicationResult,
    native_publication::publish_native_once,
    pack_io::{PackIoWorker, PackJob, PreparedPack},
};
use bace_storage_codec::{PackKey, PackLimits, PackLookup, PackManifest, PackRecord};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[tokio::test]
async fn native_profiles_journal_validate_compact_and_reopen() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let base = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(PackRecord {
            key: PackKey {
                namespace: 1,
                id: 1,
            },
            schema: 1,
            value: Some(candidate(1).bytes),
        })],
        limits,
    )
    .unwrap();
    let manifest = PackManifest {
        version: 1,
        generation: 1,
        base,
        deltas: vec![],
    };
    bace_storage_codec::write_manifest(directory.path(), &manifest, limits).unwrap();
    store
        .accept_mapped(
            None,
            &MappedGeneration {
                manifest_hash: manifest.content_hash(limits).unwrap(),
                parent_hash: None,
                base_hash: manifest.base.generation,
                accepted_revision: 0,
                manifest_bytes: manifest.encode(limits).unwrap(),
            },
        )
        .await
        .unwrap();
    let mut active = PreparedPack {
        generation: Arc::new(manifest.open(directory.path(), limits).unwrap()),
        manifest,
    };
    let old_reader = active.generation.clone();
    let initial_manifest = active.manifest.clone();
    let pressure = Arc::new(AtomicBool::new(false));
    let mut worker =
        PackIoWorker::start(directory.path().to_path_buf(), 2, pressure.clone()).unwrap();
    let (delivery, mut receiver) = tokio::sync::mpsc::channel(1);
    let profile =
        bace_content_tools::parse_rare_profile("schema_version=1\nid=1\nenabled=false\ntiers=[]\n")
            .unwrap();
    let good = NativeContentCandidate {
        namespace: 47,
        id: 1,
        schema: 1,
        bytes: bace_content_tools::compile_rare_profile(&profile).unwrap(),
    };
    let mut bad = good.clone();
    bad.id = 2;
    let rejected = store.insert_native_candidates(&[bad]).await.unwrap();
    assert!(store.pending_publications(0, 1).await.is_err());
    assert!(store.accept_validated(rejected).await.is_err());
    assert!(
        matches!(publish_native_once(&store,&mut worker,&mut active,&delivery).await.unwrap(),PublicationResult::Rejected{revision,..} if revision==rejected)
    );
    assert!(
        store
            .native_content_revision(47, 2)
            .await
            .unwrap()
            .is_none()
    );
    let missing_graph = bace_content_tools::parse_loot_graph(
        r#"
schema_version=1
id=1
root="item"
[[nodes]]
id="item"
selection="item"
branches=[]
[nodes.item]
template=2
minimum_stack=1
maximum_stack=1
"#,
    )
    .unwrap();
    let missing = NativeContentCandidate {
        namespace: 46,
        id: 1,
        schema: 1,
        bytes: bace_content_tools::compile_loot_graph(&missing_graph).unwrap(),
    };
    let rejected = store
        .insert_native_candidates(&[good.clone(), missing])
        .await
        .unwrap();
    assert!(
        matches!(publish_native_once(&store,&mut worker,&mut active,&delivery).await.unwrap(),PublicationResult::Rejected{revision,..} if revision==rejected)
    );
    assert!(
        store
            .native_content_revision(47, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .native_content_revision(46, 1)
            .await
            .unwrap()
            .is_none()
    );
    let mut final_revision = 0;
    for iteration in 0..3 {
        let revision = store
            .insert_native_candidates(std::slice::from_ref(&good))
            .await
            .unwrap();
        if iteration == 0 {
            // A cancelled earlier caller left its admitted Open completion in
            // the FIFO. It must never be mistaken for this publication's delta.
            assert!(
                worker
                    .try_submit(PackJob::Open {
                        manifest: active.manifest.clone()
                    })
                    .is_ok()
            );
            let error = publish_native_once(&store, &mut worker, &mut active, &delivery)
                .await
                .unwrap_err();
            assert!(error.contains("stale pack completion"), "{error}");
            assert_eq!(store.pending_revision().await.unwrap(), Some(revision));
            assert!(
                store
                    .native_content_revision(47, 1)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(active.manifest, initial_manifest);
            assert_eq!(worker.shutdown().unwrap().len(), 1);
            worker =
                PackIoWorker::start(directory.path().to_path_buf(), 2, pressure.clone()).unwrap();
        }
        if iteration == 2 {
            pressure.store(true, Ordering::Relaxed);
            assert!(
                publish_native_once(&store, &mut worker, &mut active, &delivery)
                    .await
                    .is_err()
            );
            assert_eq!(store.pending_revision().await.unwrap(), Some(revision));
            assert_eq!(active.manifest.deltas.len(), 2);
            pressure.store(false, Ordering::Relaxed);
        }
        assert_eq!(
            publish_native_once(&store, &mut worker, &mut active, &delivery)
                .await
                .unwrap(),
            PublicationResult::Accepted { revision }
        );
        assert!(active.manifest.deltas.len() <= 2);
        let current = receiver.recv().await.unwrap();
        let PackLookup::Record(record) = current
            .lookup(PackKey {
                namespace: 47,
                id: 1,
            })
            .unwrap()
        else {
            panic!("published profile absent")
        };
        assert_eq!(record.bytes(), good.bytes);
        assert_eq!(
            store.native_content_revision(47, 1).await.unwrap(),
            Some(revision)
        );
        final_revision = revision;
    }
    assert_eq!(active.manifest.deltas.len(), 1);
    assert!(matches!(
        old_reader
            .lookup(PackKey {
                namespace: 47,
                id: 1
            })
            .unwrap(),
        PackLookup::Missing
    ));
    let accepted = store.active_generation().await.unwrap().unwrap();
    assert_eq!(accepted.accepted_revision, final_revision);
    let reopened = PackManifest::decode(&accepted.manifest_bytes, limits)
        .unwrap()
        .open(directory.path(), limits)
        .unwrap();
    assert!(matches!(
        reopened
            .lookup(PackKey {
                namespace: 47,
                id: 1
            })
            .unwrap(),
        PackLookup::Record(_)
    ));
    // Even with an empty journal, a lost post-COMMIT delivery must be detected.
    let mut stale = PreparedPack {
        manifest: initial_manifest,
        generation: old_reader,
    };
    assert!(
        publish_native_once(&store, &mut worker, &mut stale, &delivery)
            .await
            .unwrap_err()
            .contains("stale native publication base")
    );
    assert!(worker.shutdown().unwrap().is_empty());
    store.close().await;
}

#[tokio::test]
async fn direct_sql_profiles_share_journal_and_mixed_batches_cannot_partially_publish() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let first:i64=sqlx::query_scalar("INSERT INTO native_content_candidates(namespace,content_id,schema_version,payload) VALUES(46,1,1,$1) RETURNING revision").bind(b"unvalidated".as_slice()).fetch_one(&mut *tx).await.unwrap();
    let second:i64=sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES(1,'mixed',1,$1) RETURNING revision").bind(b"unvalidated".as_slice()).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(first, second);
    tx.commit().await.unwrap();
    let pending = store.pending_native_publication().await.unwrap().unwrap();
    assert_eq!(pending.revision, first);
    assert_eq!(pending.candidates.len(), 1);
    assert_eq!(pending.weenie_candidates, 1);
    assert!(store.pending_publications(0, 1).await.is_err());
    assert!(store.accept_validated(first).await.is_err());
    assert!(
        sqlx::query("UPDATE native_content_candidates SET payload=$1")
            .bind(b"changed".as_slice())
            .execute(&pool)
            .await
            .is_err()
    );
    store
        .reject(first, "explicit mixed batch rejection")
        .await
        .unwrap();
    assert!(store.active_content().await.unwrap().is_empty());
    assert!(
        store
            .native_content_revision(46, 1)
            .await
            .unwrap()
            .is_none()
    );
    pool.close().await;
    store.close().await;
}
