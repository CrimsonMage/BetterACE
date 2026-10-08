use bace_content::WorldRecordV1;
use bace_import::{MariaDbBinaries, MariaDbStaging, import_complete_world};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn backend() -> MariaDbStaging {
    let basedir = PathBuf::from(
        std::env::var_os("BACE_MARIADB_BASEDIR").expect("set private BACE_MARIADB_BASEDIR"),
    );
    MariaDbStaging::new(MariaDbBinaries {
        install_db: basedir.join("bin/mariadb-install-db"),
        server: basedir.join("bin/mariadbd"),
        client: basedir.join("bin/mariadb"),
        basedir,
        library_dir: None,
    })
}
#[test]
#[ignore = "requires private BACE_MARIADB_BASEDIR"]
fn typed_non_weenie_rows_preserve_original_ids_boolean_null_signed_values_and_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.sql");
    // Literal independent SQL values test schema semantics through the real parser.
    let mut sql = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/fixtures/content/upstream-arrow.sql"),
    )
    .unwrap();
    sql.push_str("\nINSERT INTO event(id,name,start_Time,end_Time,state) VALUES(73,'Snow 雪',-1,-1,0);\nINSERT INTO quest(id,name,min_Delta,max_Solves,message) VALUES(51,'one',4294967295,-1,NULL);\nINSERT INTO landblock_instance(guid,weenie_Class_Id,obj_Cell_Id,origin_X,origin_Y,origin_Z,angles_W,angles_X,angles_Y,angles_Z,is_Link_Child) VALUES(2147488473,300,2724208641,1.25,2,3,1,0,0,0,b'1');\nINSERT INTO treasure_gem_count(id,gem_Code,tier,count,chance) VALUES(81,255,8,-1,0.25);\n");
    std::fs::write(&path, sql).unwrap();
    let world = import_complete_world(&mut backend(), &path).unwrap();
    assert_eq!(world.weenies.len(), 1);
    assert_eq!(world.records.len(), 4);
    let event = world
        .records
        .iter()
        .find_map(|r| {
            if let WorldRecordV1::Event(r) = r {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        (event.id, event.name.as_str(), event.start_time),
        (73, "Snow 雪", -1)
    );
    let quest = world
        .records
        .iter()
        .find_map(|r| {
            if let WorldRecordV1::Quest(r) = r {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(quest.min_delta, u32::MAX);
    assert_eq!(quest.max_solves, -1);
    assert_eq!(quest.message, None);
    let instance = world
        .records
        .iter()
        .find_map(|r| {
            if let WorldRecordV1::LandblockInstance(r) = r {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(instance.is_link_child);
    assert_eq!(instance.origin_x, 1.25);
    assert_eq!(instance.landblock, (instance.obj_cell_id >> 16) as i32);
    let gem = world
        .records
        .iter()
        .find_map(|r| {
            if let WorldRecordV1::TreasureGemCount(r) = r {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!((gem.gem_code, gem.count, gem.chance), (255, -1, 0.25));
    let pack = bace_content_tools::build_world_pack(
        &world.weenies,
        &world.records,
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(pack.records, 8);
    // Unknown populated tables still fail whole-import acceptance.
    let mut sql = std::fs::read_to_string(&path).unwrap();
    sql.push_str("CREATE TABLE not_supported(id int); INSERT INTO not_supported VALUES(1);");
    std::fs::write(&path, sql).unwrap();
    assert!(import_complete_world(&mut backend(), &path).is_err());
}
#[test]
#[ignore = "requires BACE_WORLD_SQL (verified pinned complete release) and private BACE_MARIADB_BASEDIR"]
fn unchanged_pinned_complete_release_extracts_every_table_and_builds_one_pack() {
    let path = PathBuf::from(std::env::var_os("BACE_WORLD_SQL").expect("set BACE_WORLD_SQL"));
    let world = import_complete_world(&mut backend(), &path).unwrap();
    assert_eq!(
        world.manifest.source_sha256,
        "98caa038a27d2620bc4b644e62150aa4eec7754251c6313aaa959c3b11f784a8"
    );
    assert_eq!(world.weenies.len(), 43913);
    assert_eq!(world.records.len(), 915050);
    assert_eq!(world.manifest.table_row_counts.len(), 54);
    let references = bace_content::inspect_world_references(&world.weenies, &world.records);
    assert_eq!(references.checked, 1_254_236);
    assert_eq!(references.missing, 171);
    assert_eq!(
        references.missing_by_field["landblock_instance.weenie_class_id"],
        3
    );
    let dir = tempfile::tempdir().unwrap();
    let pack = bace_content_tools::build_world_pack(
        &world.weenies,
        &world.records,
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(pack.records, 1011979);
    let manifest = bace_storage_codec::load_manifest(&pack.manifest, Default::default()).unwrap();
    let mapped =
        bace_storage_codec::MappedPack::open(dir.path(), &manifest.base, Default::default())
            .unwrap();
    bace_content_tools::validate_world_pack_indexes(&mapped).unwrap();
    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|e| e == "bace"))
            .count(),
        1
    );
}
