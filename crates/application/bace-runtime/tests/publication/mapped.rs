use super::Cluster;
use bace_content::{LandblockInstanceRowV1, QuestRowV1, WorldRecordV1};
use bace_db_postgres::PgStore;
use bace_persistence::{MappedContentCandidate, MappedGeneration};
use bace_runtime::{
    PublicationResult,
    native_publication::publish_native_once,
    pack_io::{PackIoWorker, PreparedPack},
};
use bace_storage_codec::{PackKey, PackLookup};
use std::sync::{Arc, atomic::AtomicBool};

#[tokio::test]
async fn world_row_move_and_explicit_removal_publish_as_small_deltas() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let template = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let mut instance = LandblockInstanceRowV1 {
        guid: 0x800012d9,
        landblock: 0xa260,
        weenie_class_id: 1,
        obj_cell_id: 0xa2600001,
        origin_x: 10.0,
        origin_y: 20.0,
        origin_z: 30.0,
        angles_w: 1.0,
        angles_x: 0.0,
        angles_y: 0.0,
        angles_z: 0.0,
        is_link_child: false,
        last_modified: "2026-10-08".into(),
    };
    let built = bace_content_tools::build_world_pack(
        &[template],
        &[WorldRecordV1::LandblockInstance(instance.clone())],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let initial_hash = manifest.content_hash(Default::default()).unwrap();
    store
        .accept_mapped(
            None,
            &MappedGeneration {
                manifest_hash: initial_hash,
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
    let old_reader = active.generation.clone();
    let mut worker =
        PackIoWorker::start(directory.path().into(), 1, Arc::new(AtomicBool::new(false))).unwrap();
    let (delivery, mut received) = tokio::sync::mpsc::channel(1);
    instance.landblock = 0xa261;
    instance.obj_cell_id = 0xa2610001;
    let quest = WorldRecordV1::Quest(QuestRowV1 {
        id: 7,
        name: "Test quest".into(),
        min_delta: 0,
        max_solves: 1,
        message: None,
        last_modified: "2026-10-08".into(),
    });
    let revision = store
        .insert_mapped_batch(
            &[],
            &[],
            &[
                MappedContentCandidate {
                    namespace: 20,
                    id: u64::from(instance.guid),
                    schema: 1,
                    bytes: Some(
                        bace_content_tools::compile_world_record(
                            &WorldRecordV1::LandblockInstance(instance),
                        )
                        .unwrap(),
                    ),
                },
                MappedContentCandidate {
                    namespace: 23,
                    id: 7,
                    schema: 1,
                    bytes: Some(bace_content_tools::compile_world_record(&quest).unwrap()),
                },
            ],
            initial_hash,
        )
        .await
        .unwrap();
    assert_eq!(
        publish_native_once(&store, &mut worker, &mut active, &delivery)
            .await
            .unwrap(),
        PublicationResult::Accepted { revision }
    );
    received.recv().await.unwrap();
    assert!(matches!(
        old_reader
            .lookup(PackKey {
                namespace: 23,
                id: 7
            })
            .unwrap(),
        PackLookup::Missing
    ));
    assert!(matches!(
        active
            .generation
            .lookup(PackKey {
                namespace: 23,
                id: 7
            })
            .unwrap(),
        PackLookup::Record(_)
    ));
    assert!(matches!(
        active
            .generation
            .lookup(PackKey {
                namespace: 2,
                id: 0xa260
            })
            .unwrap(),
        PackLookup::Tombstone
    ));
    let PackLookup::Record(index) = active
        .generation
        .lookup(PackKey {
            namespace: 2,
            id: 0xa261,
        })
        .unwrap()
    else {
        panic!("new landblock index missing")
    };
    assert_eq!(
        bace_content_tools::decode_landblock_index(index.bytes())
            .unwrap()
            .instance_ids,
        vec![0x800012d9]
    );
    assert_eq!(
        store.mapped_content_revision(23, 7).await.unwrap(),
        Some(revision)
    );
    let next_hash = active.manifest.content_hash(Default::default()).unwrap();
    let removal = store
        .insert_mapped_batch(
            &[],
            &[],
            &[MappedContentCandidate {
                namespace: 23,
                id: 7,
                schema: 1,
                bytes: None,
            }],
            next_hash,
        )
        .await
        .unwrap();
    assert_eq!(
        publish_native_once(&store, &mut worker, &mut active, &delivery)
            .await
            .unwrap(),
        PublicationResult::Accepted { revision: removal }
    );
    received.recv().await.unwrap();
    assert!(matches!(
        active
            .generation
            .lookup(PackKey {
                namespace: 23,
                id: 7
            })
            .unwrap(),
        PackLookup::Tombstone
    ));
    assert_eq!(store.mapped_content_revision(23, 7).await.unwrap(), None);
    assert!(matches!(
        old_reader
            .lookup(PackKey {
                namespace: 20,
                id: 0x800012d9
            })
            .unwrap(),
        PackLookup::Record(_)
    ));
}
