use bace_import::{
    SqlStagingBackend, SqlStagingError, StagedWorld, StagingManifest, import_staged_world,
    import_weenie_json,
};
use std::{collections::BTreeMap, path::Path};

struct FixtureBackend {
    extra_table: bool,
    wrong_count: bool,
}
impl SqlStagingBackend for FixtureBackend {
    fn extract_isolated(&mut self, _: &Path) -> Result<StagedWorld, SqlStagingError> {
        let mut counts = BTreeMap::from([("weenie".into(), if self.wrong_count { 2 } else { 1 })]);
        if self.extra_table {
            counts.insert("quest".into(), 1);
        }
        Ok(StagedWorld {
            manifest: StagingManifest {
                source_sha256: "0".repeat(64),
                source_release: "synthetic".into(),
                upstream_pin: "47edade3bd3f6044b676d4eb877c4965c7eda62b".into(),
                table_row_counts: counts,
                unsupported_tables: vec![],
            },
            weenies: vec![import_weenie_json(r#"{"wcid":900003,"weenieType":1}"#).unwrap()],
        })
    }
}

#[test]
fn staging_contract_rejects_silent_table_loss_and_row_count_mismatch() {
    let dump = tempfile::NamedTempFile::new().unwrap();
    let mut backend = FixtureBackend {
        extra_table: true,
        wrong_count: false,
    };
    assert!(matches!(
        import_staged_world(&mut backend, dump.path()),
        Err(SqlStagingError::UnsupportedTables(_))
    ));
    backend.extra_table = false;
    backend.wrong_count = true;
    assert!(matches!(
        import_staged_world(&mut backend, dump.path()),
        Err(SqlStagingError::Extraction(_))
    ));
    backend.wrong_count = false;
    assert_eq!(
        import_staged_world(&mut backend, dump.path())
            .unwrap()
            .weenies
            .len(),
        1
    );
}
