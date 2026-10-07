use bace_import::{MariaDbBinaries, MariaDbStaging, import_staged_world};
use std::path::PathBuf;

#[test]
#[ignore = "requires an explicitly configured private MariaDB installation; run with BACE_MARIADB_BASEDIR"]
fn unchanged_official_weenie_sql_is_executed_in_private_instance() {
    let mut backend = backend();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/content/upstream-arrow.sql");
    let world = import_staged_world(&mut backend, &input).unwrap();
    assert_eq!(world.weenies.len(), 1);
    let arrow = &world.weenies[0];
    assert_eq!(arrow.weenie_id, 300);
    assert_eq!(arrow.class_name, "arrow");
    assert!(
        arrow
            .properties
            .strings
            .iter()
            .any(|p| p.id == 1 && p.value == "Arrow")
    );
    assert_eq!(world.manifest.table_row_counts["weenie"], 1);
    let binary = bace_content_tools::compile_template(arrow).unwrap();
    assert_eq!(arrow, &bace_content_tools::decode(&binary).unwrap());
}

fn backend() -> MariaDbStaging {
    let basedir = PathBuf::from(
        std::env::var_os("BACE_MARIADB_BASEDIR")
            .expect("set BACE_MARIADB_BASEDIR to a private MariaDB installation"),
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
#[ignore = "requires BACE_MARIADB_BASEDIR private MariaDB installation"]
fn complex_sql_preserves_order_nulls_unsigned_motion_and_session_variables() {
    let mut backend = backend();
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/content/complex-weenie.sql");
    let world = import_staged_world(&mut backend, &input).unwrap();
    let template = &world.weenies[0];
    let p = &template.properties;
    assert!(p.strings[0].value.contains("'quotes', ; semicolon and 雪"));
    assert_eq!(p.book_pages[0].legacy_page_id, Some(10));
    assert_eq!(p.book_pages[1].legacy_page_id, Some(20));
    assert_eq!(p.emotes[0].actions[0].legacy_order, Some(10));
    assert_eq!(p.emotes[0].actions[0].motion, Some(0x85000001));
    assert_eq!(p.emotes[0].actions[0].display, Some(false));
    assert_eq!(p.emotes[0].actions[1].display, None);
    assert_eq!(p.emotes[0].actions[1].hero_xp64, Some(i64::MAX));
    assert_eq!(p.generators[0].delay, None);
    assert!(p.create_list[0].try_to_bond);
    assert_eq!(p.positions[0].value.position_x, 1.0);
    assert_eq!(p.skills[0].value.pp, u32::MAX);
    assert_eq!(p.texture_maps[0].new_texture, 456);
    let binary = bace_content_tools::compile_template(template).unwrap();
    assert_eq!(
        template,
        &bace_content_tools::parse(&bace_content_tools::export_binary(&binary).unwrap()).unwrap()
    );
}

#[test]
#[ignore = "requires BACE_MARIADB_BASEDIR private MariaDB installation"]
fn failed_or_lossy_sql_cannot_produce_partial_import() {
    use std::io::Write;
    let base = include_str!("../../../../tests/fixtures/content/upstream-arrow.sql");
    for tail in [
        "INSERT INTO absent_table VALUES (1);",
        "CREATE TABLE future_data (value INT); INSERT INTO future_data VALUES (1);",
        "SET sql_mode=''; INSERT INTO weenie_properties_int (object_Id,type,value) VALUES (300,65000,999999999999999999);",
        "ALTER TABLE weenie ADD unknown_column INT;",
    ] {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, "{base}\n{tail}").unwrap();
        assert!(
            import_staged_world(&mut backend(), file.path()).is_err(),
            "accepted {tail}"
        );
    }
}
