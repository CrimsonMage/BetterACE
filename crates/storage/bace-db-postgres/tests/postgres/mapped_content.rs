use super::Cluster;
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{MappedContentCandidate, MappedGeneration};
use bace_storage_codec::{PackKey, PackLimits, PackManifest, PackRecord};

#[tokio::test]
async fn reviewed_mapped_batch_is_atomic_and_fenced_to_accepted_pack() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let base = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(PackRecord {
            key: PackKey {
                namespace: 23,
                id: 2,
            },
            schema: 1,
            value: Some(vec![1]),
        })],
        limits,
    )
    .unwrap();
    let manifest = PackManifest {
        version: 1,
        generation: 1,
        base,
        deltas: Vec::new(),
    };
    bace_storage_codec::write_manifest(directory.path(), &manifest, limits).unwrap();
    let original = manifest.content_hash(limits).unwrap();
    store
        .accept_mapped(
            None,
            &MappedGeneration {
                manifest_hash: original,
                parent_hash: None,
                base_hash: manifest.base.generation,
                accepted_revision: 0,
                manifest_bytes: manifest.encode(limits).unwrap(),
            },
        )
        .await
        .unwrap();
    let candidate = MappedContentCandidate {
        namespace: 23,
        id: 3,
        schema: 1,
        bytes: Some(vec![2]),
    };
    assert!(matches!(
        store
            .insert_mapped_batch(&[], &[], std::slice::from_ref(&candidate), [9; 32])
            .await,
        Err(StoreError::GenerationConflict)
    ));
    let revision = store
        .insert_mapped_batch(&[], &[], &[candidate], original)
        .await
        .unwrap();
    let pending = store.pending_native_publication().await.unwrap().unwrap();
    assert_eq!(pending.revision, revision);
    assert_eq!(
        store
            .publication_status(revision)
            .await
            .unwrap()
            .unwrap()
            .status,
        "pending"
    );
    assert_eq!(pending.expected_manifest_hash, Some(original));
    assert_eq!(pending.mapped_candidates.len(), 1);
    assert!(
        store
            .insert_mapped_batch(
                &[],
                &[],
                &[MappedContentCandidate {
                    namespace: 23,
                    id: 4,
                    schema: 1,
                    bytes: Some(vec![3]),
                }],
                original
            )
            .await
            .is_err()
    );
    let delta = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(PackRecord {
            key: PackKey {
                namespace: 23,
                id: 3,
            },
            schema: 1,
            value: Some(vec![2]),
        })],
        limits,
    )
    .unwrap();
    let accepted = PackManifest {
        version: 1,
        generation: 2,
        base: manifest.base,
        deltas: vec![delta],
    };
    bace_storage_codec::write_manifest(directory.path(), &accepted, limits).unwrap();
    store
        .accept_mapped(
            Some(revision),
            &MappedGeneration {
                manifest_hash: accepted.content_hash(limits).unwrap(),
                parent_hash: Some(original),
                base_hash: accepted.base.generation,
                accepted_revision: revision,
                manifest_bytes: accepted.encode(limits).unwrap(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.mapped_content_revision(23, 3).await.unwrap(),
        Some(revision)
    );
    assert_eq!(
        store
            .publication_status(revision)
            .await
            .unwrap()
            .unwrap()
            .status,
        "accepted"
    );
    assert_eq!(
        store
            .pending_native_publication()
            .await
            .unwrap()
            .map(|p| p.revision),
        None
    );
}
