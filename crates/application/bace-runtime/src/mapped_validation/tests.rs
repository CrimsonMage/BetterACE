use super::validate_mapped;
use bace_content::{LandblockInstanceRowV1, WorldRecordV1};
use bace_persistence::MappedContentCandidate;
use bace_storage_codec::{PackKey, PackLimits};
use std::sync::atomic::AtomicBool;

#[test]
fn moving_an_instance_updates_both_landblock_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let weenie = bace_content_tools::parse(
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
        &[weenie],
        &[WorldRecordV1::LandblockInstance(instance.clone())],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let generation = bace_storage_codec::load_manifest(&built.manifest, PackLimits::default())
        .unwrap()
        .open(directory.path(), PackLimits::default())
        .unwrap();
    instance.landblock = 0xa261;
    instance.obj_cell_id = 0xa2610001;
    let bytes =
        bace_content_tools::compile_world_record(&WorldRecordV1::LandblockInstance(instance))
            .unwrap();
    let (records, removed, references) = validate_mapped(
        &generation,
        vec![MappedContentCandidate {
            namespace: 20,
            id: 0x800012d9,
            schema: 1,
            bytes: Some(bytes),
        }],
    )
    .unwrap();
    assert!(removed.is_empty());
    assert_eq!(references, vec![1]);
    assert_eq!(
        records
            .iter()
            .find(|r| r.key
                == PackKey {
                    namespace: 2,
                    id: 0xa260
                })
            .unwrap()
            .value,
        None
    );
    let added = records
        .iter()
        .find(|r| {
            r.key
                == PackKey {
                    namespace: 2,
                    id: 0xa261,
                }
        })
        .unwrap();
    let index = bace_content_tools::decode_landblock_index(added.value.as_ref().unwrap()).unwrap();
    assert_eq!(index.instance_ids, vec![0x800012d9]);
}
