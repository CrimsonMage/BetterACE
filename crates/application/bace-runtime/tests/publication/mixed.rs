//! Real journal/pack transactions prove atomic derived indexes and references.
use super::Cluster;
use bace_content::{Property, WeenieV1};
use bace_db_postgres::PgStore;
use bace_persistence::{ContentCandidate, MappedGeneration, NativeContentCandidate};
use bace_runtime::{
    PublicationResult,
    native_publication::publish_native_once,
    pack_io::{PackIoWorker, PreparedPack},
};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::sync::{Arc, atomic::AtomicBool};

fn template(id: u32, class_name: &str, kind: u32, name: &str) -> WeenieV1 {
    let mut row = WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: class_name.into(),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    };
    row.properties.strings.push(Property {
        id: 1,
        value: name.into(),
    });
    row
}
fn candidate(row: &WeenieV1) -> ContentCandidate {
    ContentCandidate {
        wcid: row.weenie_id,
        class_name: row.class_name.clone(),
        weenie_type: row.weenie_type as i32,
        bytes: bace_content_tools::compile_template(row).unwrap(),
    }
}
fn names(pack: &PackGeneration) -> Vec<(u32, String)> {
    let PackLookup::Record(record) = pack
        .lookup(PackKey {
            namespace: 48,
            id: 1,
        })
        .unwrap()
    else {
        panic!("names missing")
    };
    bace_content_tools::decode_creature_names(record.bytes())
        .unwrap()
        .entries
        .into_iter()
        .map(|r| (r.template, r.name))
        .collect()
}
async fn journal(
    pool: &sqlx::PgPool,
    rows: &[ContentCandidate],
    native: Option<&NativeContentCandidate>,
) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    let mut revision = None;
    for row in rows {
        let next: i64 = sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES($1,$2,$3,$4) RETURNING revision")
            .bind(i64::from(row.wcid)).bind(&row.class_name).bind(row.weenie_type).bind(&row.bytes).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(*revision.get_or_insert(next), next);
    }
    if let Some(row) = native {
        let next: i64 = sqlx::query_scalar("INSERT INTO native_content_candidates(namespace,content_id,schema_version,payload) VALUES($1,$2,$3,$4) RETURNING revision")
            .bind(i32::from(row.namespace)).bind(i64::from(row.id)).bind(i32::from(row.schema)).bind(&row.bytes).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(*revision.get_or_insert(next), next);
    }
    tx.commit().await.unwrap();
    revision.unwrap()
}

#[tokio::test]
async fn mixed_publication_preserves_names_and_resolves_new_template_references_atomically() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let initial = [
        template(1, "one", 10, "Drudge"),
        template(2, "two", 10, "Drudge"),
    ];
    let built = bace_content_tools::build_world_pack(
        &initial,
        &[],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    store
        .accept_mapped(
            None,
            &MappedGeneration {
                manifest_hash: manifest.content_hash(Default::default()).unwrap(),
                parent_hash: None,
                base_hash: manifest.base.generation,
                accepted_revision: 0,
                manifest_bytes: manifest.encode(Default::default()).unwrap(),
            },
        )
        .await
        .unwrap();
    let mut active = PreparedPack {
        generation: Arc::new(manifest.open(directory.path(), Default::default()).unwrap()),
        manifest,
    };
    let original = active.generation.clone();
    let mut worker =
        PackIoWorker::start(directory.path().into(), 1, Arc::new(AtomicBool::new(false))).unwrap();
    let (delivery, mut receiver) = tokio::sync::mpsc::channel(1);
    let graph = bace_content_tools::parse_loot_graph("schema_version=1\nid=1\nroot=\"item\"\n[[nodes]]\nid=\"item\"\nselection=\"item\"\nbranches=[]\n[nodes.item]\ntemplate=3\nminimum_stack=1\nmaximum_stack=1\n").unwrap();
    let native = NativeContentCandidate {
        namespace: 46,
        id: 1,
        schema: 1,
        bytes: bace_content_tools::compile_loot_graph(&graph).unwrap(),
    };
    // Collision with a template in the base pack is rejected even though that
    // template has no SQL delta head. Neither half of the transaction publishes.
    let bad = journal(
        &pool,
        &[candidate(&template(3, "two", 10, "Tumerok"))],
        Some(&native),
    )
    .await;
    assert!(
        matches!(publish_native_once(&store, &mut worker, &mut active, &delivery).await.unwrap(), PublicationResult::Rejected { revision, .. } if revision == bad)
    );
    assert!(receiver.try_recv().is_err());
    assert_eq!(store.native_content_revision(46, 1).await.unwrap(), None);
    assert!(store.active_content().await.unwrap().is_empty());
    assert_eq!(
        names(&active.generation),
        vec![(1, "Drudge".into()), (2, "Drudge".into())]
    );
    // One transaction swaps class identities, removes one creature name by
    // changing its type, and adds a new template referenced by the loot graph.
    let accepted = journal(
        &pool,
        &[
            candidate(&template(1, "two", 1, "Object")),
            candidate(&template(2, "one", 10, "Renamed Drudge")),
            candidate(&template(3, "three", 10, "Tumerok")),
        ],
        Some(&native),
    )
    .await;
    assert_eq!(
        publish_native_once(&store, &mut worker, &mut active, &delivery)
            .await
            .unwrap(),
        PublicationResult::Accepted { revision: accepted }
    );
    let delivered = receiver.recv().await.unwrap();
    assert_eq!(
        names(&delivered),
        vec![(2, "Renamed Drudge".into()), (3, "Tumerok".into())]
    );
    assert_eq!(
        names(&original),
        vec![(1, "Drudge".into()), (2, "Drudge".into())]
    );
    assert_eq!(
        store.native_content_revision(46, 1).await.unwrap(),
        Some(accepted)
    );
    assert!(matches!(
        delivered
            .lookup(PackKey {
                namespace: 46,
                id: 1
            })
            .unwrap(),
        PackLookup::Record(_)
    ));
    // A second swap exercises SQL head replacement, not just the base index.
    let swapped = journal(
        &pool,
        &[candidate(&initial[0]), candidate(&initial[1])],
        None,
    )
    .await;
    assert_eq!(
        publish_native_once(&store, &mut worker, &mut active, &delivery)
            .await
            .unwrap(),
        PublicationResult::Accepted { revision: swapped }
    );
    receiver.recv().await.unwrap();
    let durable = store.active_generation().await.unwrap().unwrap();
    let reopened =
        bace_storage_codec::PackManifest::decode(&durable.manifest_bytes, Default::default())
            .unwrap()
            .open(directory.path(), Default::default())
            .unwrap();
    assert_eq!(
        names(&reopened),
        vec![
            (1, "Drudge".into()),
            (2, "Drudge".into()),
            (3, "Tumerok".into())
        ]
    );
    assert_eq!(
        store
            .generation_by_hash(durable.manifest_hash)
            .await
            .unwrap()
            .unwrap()
            .manifest_bytes,
        durable.manifest_bytes
    );
    assert!(worker.shutdown().unwrap().is_empty());
    pool.close().await;
    store.close().await;
}
