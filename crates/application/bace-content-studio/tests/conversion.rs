use bace_content_studio::{ConversionOptions, convert_file};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/content")
        .join(name)
}

fn options(output: &Path) -> ConversionOptions {
    ConversionOptions {
        output_parent: output.to_path_buf(),
        mariadb_basedir: None,
    }
}

#[test]
fn lifestoned_export_preserves_authored_values_and_never_overwrites() {
    let output = tempfile::tempdir().unwrap();
    let source = fixture("lifestoned-weenie.json");
    let first = convert_file(&source, &options(output.path()), &AtomicBool::new(false)).unwrap();
    assert_eq!(first.weenies, 1);
    let file = first.directory.join("900003.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    let template = bace_content_tools::parse(&text).unwrap();
    assert_eq!(template.weenie_id, 900003);
    assert!(
        template
            .properties
            .strings
            .iter()
            .any(|p| p.id == 1 && p.value == "Fixture NPC 雪")
    );
    assert!(
        template
            .properties
            .int64s
            .iter()
            .any(|p| p.id == 90000 && p.value == i64::MAX)
    );
    let manifest: toml::Table =
        toml::from_str(&std::fs::read_to_string(first.directory.join("manifest.toml")).unwrap())
            .unwrap();
    assert_eq!(manifest["weenie_count"].as_integer(), Some(1));
    assert_eq!(manifest["source_sha256"].as_str().unwrap().len(), 64);
    std::fs::write(&file, "user-edited output").unwrap();
    let second = convert_file(&source, &options(output.path()), &AtomicBool::new(false)).unwrap();
    assert_ne!(first.directory, second.directory);
    assert_eq!(std::fs::read_to_string(file).unwrap(), "user-edited output");
}

#[test]
fn malformed_unknown_oversized_and_cancelled_inputs_leave_no_exports() {
    let source = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let json = source.path().join("bad.json");
    for invalid in ["{", r#"{"WeenieClassId": 1, "UnknownFutureField": 7}"#] {
        std::fs::write(&json, invalid).unwrap();
        assert!(convert_file(&json, &options(output.path()), &AtomicBool::new(false)).is_err());
    }
    let file = std::fs::File::create(&json).unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    assert!(
        convert_file(&json, &options(output.path()), &AtomicBool::new(false))
            .unwrap_err()
            .contains("16 MiB")
    );
    assert!(
        convert_file(
            &fixture("lifestoned-weenie.json"),
            &options(output.path()),
            &AtomicBool::new(true)
        )
        .unwrap_err()
        .contains("Cancelled")
    );
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 0);
}

#[test]
fn sql_without_setup_reports_actionable_error_without_output() {
    let output = tempfile::tempdir().unwrap();
    let error = convert_file(
        &fixture("upstream-arrow.sql"),
        &options(output.path()),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.contains("MariaDB") || error.contains("Linux"));
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 0);
}

#[test]
#[ignore = "requires BACE_MARIADB_BASEDIR pointing at a private Linux MariaDB installation"]
fn official_sql_converts_through_studio_and_lossy_sql_leaves_no_export() {
    let output = tempfile::tempdir().unwrap();
    let mut options = options(output.path());
    options.mariadb_basedir = Some(
        std::env::var_os("BACE_MARIADB_BASEDIR")
            .expect("private MariaDB prerequisite")
            .into(),
    );
    let result = convert_file(
        &fixture("upstream-arrow.sql"),
        &options,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.weenies, 1);
    let template = bace_content_tools::parse(
        &std::fs::read_to_string(result.directory.join("300.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(template.class_name, "arrow");
    assert!(
        template
            .properties
            .strings
            .iter()
            .any(|p| p.id == 1 && p.value == "Arrow")
    );
    let manifest: toml::Table =
        toml::from_str(&std::fs::read_to_string(result.directory.join("manifest.toml")).unwrap())
            .unwrap();
    assert_eq!(manifest["table_row_counts"]["weenie"].as_integer(), Some(1));
    let source = tempfile::tempdir().unwrap();
    let bad_sql = source.path().join("unsupported.sql");
    let mut sql = std::fs::read_to_string(fixture("upstream-arrow.sql")).unwrap();
    sql.push_str(
        "\nCREATE TABLE future_world_data (value INT); INSERT INTO future_world_data VALUES (1);\n",
    );
    std::fs::write(&bad_sql, sql).unwrap();
    assert!(convert_file(&bad_sql, &options, &AtomicBool::new(false)).is_err());
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 1);
}
