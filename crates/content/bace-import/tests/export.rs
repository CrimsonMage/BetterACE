//! Expected fields/columns derive from official ACE.Entity.Models and pinned
//! WorldBase.sql at 47edade3bd3f6044b676d4eb877c4965c7eda62b (AGPL-3.0-only).
use bace_import::{export_weenie_json, export_weenie_sql, import_weenie_json};

#[test]
fn ace_json_export_uses_official_names_and_exact_integer_widths() {
    let native=bace_content_tools::parse("schema_version=1\nweenie_id=300\nclass_name='arrow'\nweenie_type=3\n[[properties.int64s]]\nid=90000\nvalue=9223372036854775807\n[[properties.strings]]\nid=1\nvalue='Arrow 雪'\n").unwrap();
    let text = export_weenie_json(&native).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["WeenieClassId"], 300);
    assert_eq!(value["ClassName"], "arrow");
    assert_eq!(value["PropertiesInt64"]["90000"].as_i64(), Some(i64::MAX));
    assert_eq!(value["PropertiesString"]["1"], "Arrow 雪");
    assert!(value.get("properties").is_none());
    assert_eq!(import_weenie_json(&text).unwrap(), native);
}

#[test]
fn strict_legacy_export_rejects_unrepresentable_metadata_and_reordering() {
    let mut native = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='item'\nweenie_type=1\n",
    )
    .unwrap();
    native.last_modified = Some("2020-01-01 00:00:00".into());
    assert!(export_weenie_json(&native).is_err());
    native.properties.authoring_metadata = Some(Default::default());
    assert!(export_weenie_sql(&native).is_err());
    native.properties.authoring_metadata = None;
    native.properties.book_pages = vec![
        bace_content::BookPage {
            legacy_page_id: Some(10),
            ..Default::default()
        },
        bace_content::BookPage {
            legacy_page_id: Some(5),
            ..Default::default()
        },
    ];
    assert!(
        export_weenie_sql(&native)
            .unwrap_err()
            .to_string()
            .contains("authored sequence")
    );
}

#[test]
#[ignore = "requires BACE_MARIADB_BASEDIR private MariaDB installation"]
fn exported_complex_sql_survives_official_schema_parser_without_loss_of_authored_values() {
    use std::path::PathBuf;
    let basedir = PathBuf::from(
        std::env::var_os("BACE_MARIADB_BASEDIR").expect("private MariaDB prerequisite"),
    );
    let mut backend = bace_import::MariaDbStaging::new(bace_import::MariaDbBinaries {
        install_db: basedir.join("bin/mariadb-install-db"),
        server: basedir.join("bin/mariadbd"),
        client: basedir.join("bin/mariadb"),
        basedir,
        library_dir: None,
    });
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/content/complex-weenie.sql");
    let source = bace_import::import_staged_world(&mut backend, &input).unwrap();
    let sql = export_weenie_sql(&source.weenies[0]).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("export.sql");
    std::fs::write(&output, &sql).unwrap();
    let result = bace_import::import_staged_world(&mut backend, &output).unwrap();
    let properties = &result.weenies[0].properties;
    assert!(
        properties.strings[0]
            .value
            .contains("'quotes', ; semicolon and 雪")
    );
    assert_eq!(properties.emotes[0].actions[0].motion, Some(0x85000001));
    assert_eq!(properties.emotes[0].actions[1].hero_xp64, Some(i64::MAX));
    assert_eq!(properties.emotes[0].actions[0].display, Some(false));
    assert_eq!(properties.emotes[0].actions[1].display, None);
    assert_eq!(properties.book_pages[0].legacy_page_id, Some(10));
    assert_eq!(properties.book_pages[1].legacy_page_id, Some(20));
    let mut before = source.weenies[0].clone();
    let mut after = result.weenies[0].clone();
    // Destination relational IDs are explicitly assigned on INSERT. All other
    // fields, including legacy sequence IDs and optional values, must survive.
    for template in [&mut before, &mut after] {
        for e in &mut template.properties.emotes {
            e.database_record_id = 0;
            for a in &mut e.actions {
                a.database_record_id = 0;
            }
        }
        for c in &mut template.properties.create_list {
            c.database_record_id = 0;
        }
        for g in &mut template.properties.generators {
            g.database_record_id = 0;
        }
    }
    assert_eq!(before, after);
}
