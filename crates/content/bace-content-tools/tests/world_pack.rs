use bace_content::*;
use bace_content_tools::*;
use bace_storage_codec::*;
use std::sync::atomic::AtomicBool;
fn row() -> WorldRecordV1 {
    WorldRecordV1::LandblockInstance(LandblockInstanceRowV1 {
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
        last_modified: "2026-10-07 00:00:00".into(),
    })
}
fn bytes(generation: &PackGeneration, namespace: u16, id: u64) -> RecordHandle {
    match generation.lookup(PackKey { namespace, id }).unwrap() {
        PackLookup::Record(h) => h,
        _ => panic!("missing"),
    }
}
#[test]
fn aggregate_world_and_landblock_lookup_reopen_and_reject_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let w = parse(
        "schema_version = 1\nweenie_id = 1\nclass_name = 'test'\nweenie_type = 10\n[properties]\n",
    )
    .unwrap();
    let rows = [row()];
    let built = build_world_pack(
        std::slice::from_ref(&w),
        &rows,
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(built.records, 6);
    let generation = load_manifest(&built.manifest, PackLimits::default())
        .unwrap()
        .open(dir.path(), PackLimits::default())
        .unwrap();
    assert_eq!(
        bace_content_tools::decode(bytes(&generation, 1, 1).bytes()).unwrap(),
        w
    );
    assert_eq!(
        bace_content_tools::decode_treasure_table_set(bytes(&generation, 52, 1).bytes())
            .unwrap()
            .tables
            .len(),
        1264
    );
    assert_eq!(
        decode_world_record(bytes(&generation, rows[0].namespace(), rows[0].id()).bytes()).unwrap(),
        rows[0]
    );
    let index = decode_landblock_index(bytes(&generation, 2, 0xa260).bytes()).unwrap();
    assert_eq!(index.instance_ids, vec![0x800012d9]);
    assert!(build_world_pack(&[w], &[row(), row()], dir.path(), &AtomicBool::new(false)).is_err());
}
#[test]
fn world_codec_rejects_nonfinite_truncated_corrupt_and_wrong_schema() {
    let mut r = row();
    if let WorldRecordV1::LandblockInstance(ref mut r) = r {
        r.origin_x = f32::NAN;
    }
    assert!(compile_world_record(&r).is_err());
    let encoded = compile_world_record(&row()).unwrap();
    for n in 0..encoded.len() {
        assert!(decode_world_record(&encoded[..n]).is_err());
    }
    let mut corrupt = encoded;
    corrupt[55] ^= 1;
    assert!(decode_world_record(&corrupt).is_err());
    let wrong = encode(2, 2, &row(), CodecLimits::default()).unwrap();
    assert!(decode_world_record(&wrong).is_err());
}

#[test]
fn native_world_toml_preserves_fields_and_rejects_unknown_fields() {
    let source = export_world_record(&row()).unwrap();
    assert_eq!(parse_world_record(&source).unwrap(), row());
    assert!(parse_world_record(&(source + "extra_field = 1\n")).is_err());
}
#[test]
fn parent_link_lookup_preserves_source_ids_without_scanning_world() {
    let dir = tempfile::tempdir().unwrap();
    let w =
        parse("schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n")
            .unwrap();
    let links = [
        WorldRecordV1::LandblockInstanceLink(LandblockInstanceLinkRowV1 {
            id: 19,
            parent_guid: 5,
            child_guid: 7,
            last_modified: "2026-10-07".into(),
        }),
        WorldRecordV1::LandblockInstanceLink(LandblockInstanceLinkRowV1 {
            id: 3,
            parent_guid: 5,
            child_guid: 8,
            last_modified: "2026-10-07".into(),
        }),
    ];
    let pack = build_world_pack(&[w], &links, dir.path(), &AtomicBool::new(false)).unwrap();
    let generation = load_manifest(&pack.manifest, PackLimits::default())
        .unwrap()
        .open(dir.path(), PackLimits::default())
        .unwrap();
    let index = decode_instance_link_index(bytes(&generation, 3, 5).bytes()).unwrap();
    assert_eq!(index.parent_guid, 5);
    assert_eq!(
        index.children,
        vec![
            InstanceLinkTargetV1 {
                link_id: 3,
                child_guid: 8
            },
            InstanceLinkTargetV1 {
                link_id: 19,
                child_guid: 7
            }
        ]
    );
}
