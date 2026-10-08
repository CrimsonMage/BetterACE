use bace_content::{
    EncounterRowV1, InstanceLinkIndexV1, InstanceLinkTargetV1, LandblockIndexV1,
    LandblockInstanceLinkRowV1, LandblockInstanceRowV1, WorldRecordV1,
};
use bace_content_tools::{compile_world_record, validate_world_pack_indexes};
use bace_storage_codec::{
    CodecLimits, MappedPack, PackKey, PackLimits, PackRecord, compile_pack, encode,
};
fn record(namespace: u16, id: u64, value: Vec<u8>) -> PackRecord {
    PackRecord {
        key: PackKey { namespace, id },
        schema: 1,
        value: Some(value),
    }
}
fn region(id: u16, instances: Vec<u32>, encounters: Vec<u32>) -> PackRecord {
    record(
        2,
        u64::from(id),
        encode(
            3,
            1,
            &LandblockIndexV1 {
                landblock: id,
                instance_ids: instances,
                encounter_ids: encounters,
            },
            CodecLimits::default(),
        )
        .unwrap(),
    )
}
fn links(parent: u32, children: Vec<(u32, u32)>) -> PackRecord {
    record(
        3,
        u64::from(parent),
        encode(
            4,
            1,
            &InstanceLinkIndexV1 {
                parent_guid: parent,
                children: children
                    .into_iter()
                    .map(|(link_id, child_guid)| InstanceLinkTargetV1 {
                        link_id,
                        child_guid,
                    })
                    .collect(),
            },
            CodecLimits::default(),
        )
        .unwrap(),
    )
}
fn row(row: WorldRecordV1) -> PackRecord {
    record(
        row.namespace(),
        row.id(),
        compile_world_record(&row).unwrap(),
    )
}
fn fixture() -> Vec<PackRecord> {
    let mut records = vec![
        region(1, vec![100, 101], vec![]),
        region(2, vec![200], vec![300]),
        links(100, vec![(400, 999), (401, 200)]),
    ];
    for (guid, landblock) in [(100, 1), (101, 1), (200, 2)] {
        records.push(row(WorldRecordV1::LandblockInstance(
            LandblockInstanceRowV1 {
                guid,
                landblock,
                weenie_class_id: 7,
                obj_cell_id: ((landblock as u32) << 16) | 1,
                origin_x: 0.0,
                origin_y: 0.0,
                origin_z: 0.0,
                angles_w: 1.0,
                angles_x: 0.0,
                angles_y: 0.0,
                angles_z: 0.0,
                is_link_child: false,
                last_modified: "2026-10-07".into(),
            },
        )));
    }
    records.push(row(WorldRecordV1::Encounter(EncounterRowV1 {
        id: 300,
        landblock: 2,
        weenie_class_id: 8,
        cell_x: 1,
        cell_y: 2,
        last_modified: "2026-10-07".into(),
    })));
    for (id, child_guid) in [(400, 999), (401, 200)] {
        records.push(row(WorldRecordV1::LandblockInstanceLink(
            LandblockInstanceLinkRowV1 {
                id,
                parent_guid: 100,
                child_guid,
                last_modified: "2026-10-07".into(),
            },
        )));
    }
    records
}
fn validate(mut records: Vec<PackRecord>) -> Result<(), String> {
    records.sort_by_key(|r| r.key);
    let directory = tempfile::tempdir().unwrap();
    let limits = PackLimits::default();
    let descriptor = compile_pack(directory.path(), records.into_iter().map(Ok), limits).unwrap();
    let pack = MappedPack::open(directory.path(), &descriptor, limits).unwrap();
    validate_world_pack_indexes(&pack)
}
#[test]
fn complete_canonical_indexes_allow_authored_dangling_child_guids() {
    assert!(validate(fixture()).is_ok());
}
#[test]
fn missing_empty_and_partial_region_indexes_fail() {
    let mut records = fixture();
    records.remove(0);
    assert!(validate(records).unwrap_err().contains("incomplete"));
    let mut records = fixture();
    records[0] = region(1, vec![], vec![]);
    assert!(validate(records).unwrap_err().contains("empty"));
    let mut records = fixture();
    records[0] = region(1, vec![100], vec![]);
    assert!(validate(records).unwrap_err().contains("incomplete"));
    let mut records = fixture();
    records[1] = region(2, vec![200], vec![]);
    assert!(validate(records).unwrap_err().contains("incomplete"));
}
#[test]
fn wrong_member_misdirected_key_duplicate_and_reordered_indexes_fail() {
    for ids in [
        vec![100, 200],
        vec![100, 100, 101],
        vec![101, 100],
        vec![100, 777],
    ] {
        let mut records = fixture();
        records[0] = region(1, ids, vec![]);
        assert!(validate(records).is_err());
    }
    let mut records = fixture();
    records[0].key.id = 7;
    assert!(validate(records).unwrap_err().contains("identity"));
    let mut records = fixture();
    records[0] = region(1, vec![100, 101], vec![300]);
    assert!(validate(records).is_err());
}
#[test]
fn absent_empty_extra_and_incorrect_parent_link_indexes_fail() {
    let mut records = fixture();
    records.remove(2);
    assert!(validate(records).unwrap_err().contains("incomplete"));
    for children in [
        vec![],
        vec![(400, 999)],
        vec![(400, 200), (401, 200)],
        vec![(401, 200), (400, 999)],
        vec![(400, 999), (400, 999), (401, 200)],
    ] {
        let mut records = fixture();
        records[2] = links(100, children);
        assert!(validate(records).is_err());
    }
    let mut records = fixture();
    records[2] = links(101, vec![(400, 999), (401, 200)]);
    assert!(validate(records).is_err());
    let mut records = fixture();
    records.push(links(555, vec![]));
    assert!(validate(records).is_err());
}
#[test]
fn extra_empty_region_and_source_tombstone_and_schema_mismatch_fail() {
    let mut records = fixture();
    records.push(region(3, vec![], vec![]));
    assert!(validate(records).is_err());
    let mut records = fixture();
    records[3].value = None;
    assert!(validate(records).is_err());
    let mut records = fixture();
    records[0].schema = 2;
    assert!(validate(records).is_err());
}
#[test]
#[ignore = "requires BACE_WORLD_MANIFEST pointing to a complete compiled world base"]
fn complete_actual_world_indexes_validate_without_retaining_decoded_world() {
    let path = std::path::PathBuf::from(
        std::env::var_os("BACE_WORLD_MANIFEST").expect("set BACE_WORLD_MANIFEST"),
    );
    let limits = PackLimits::default();
    let manifest = bace_storage_codec::load_manifest(&path, limits).unwrap();
    assert!(
        manifest.deltas.is_empty(),
        "this validation accepts a complete base"
    );
    let pack = MappedPack::open(path.parent().unwrap(), &manifest.base, limits).unwrap();
    validate_world_pack_indexes(&pack).unwrap();
}
