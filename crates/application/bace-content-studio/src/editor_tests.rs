use crate::{
    document::{self, Document},
    legacy_bundle::{self, Format},
};

const SIMPLE: &str = "schema_version=1\nweenie_id=1\nclass_name='item'\nweenie_type=1\n[[properties.int64s]]\nid=90000\nvalue=9223372036854775807\n";

#[test]
fn editing_undo_and_save_preserve_full_width_values_and_detect_external_changes() {
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join("item.toml");
    document::write(&path, SIMPLE, None).unwrap();
    let mut doc = document::open(&path).unwrap();
    assert!(!doc.dirty());
    doc.value["class_name"] = toml::Value::String("edited".into());
    doc.checkpoint().unwrap();
    assert!(doc.dirty());
    doc.undo();
    assert_eq!(doc.value["class_name"].as_str(), Some("item"));
    doc.redo();
    assert_eq!(doc.value["class_name"].as_str(), Some("edited"));
    let text = doc.validated().unwrap();
    let template = bace_content_tools::parse(&text).unwrap();
    assert_eq!(template.properties.int64s[0].value, i64::MAX);
    std::fs::write(&path, "external edit").unwrap();
    assert!(
        document::write(&path, &text, doc.disk_hash)
            .unwrap_err()
            .contains("changed on disk")
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external edit");
    assert!(doc.dirty());
}

#[test]
fn duplicate_property_ids_cannot_be_saved_and_clone_does_not_target_original() {
    let mut doc = Document::from_text(SIMPLE, None, None).unwrap();
    let rows = doc.value["properties"]["int64s"].as_array_mut().unwrap();
    rows.push(rows[0].clone());
    doc.checkpoint().unwrap();
    assert!(doc.validated().is_err());
    doc.undo();
    assert!(doc.validated().is_ok());
    doc.clone_as_new();
    assert!(doc.path.is_none());
    assert!(doc.dirty());
}

#[test]
fn legacy_bundle_preserves_all_native_metadata_in_companion() {
    let source = include_str!("../../../../tests/fixtures/content/lifestoned-weenie.json");
    let original = bace_import::import_weenie_json(source).unwrap();
    let text = bace_content_tools::export(&original).unwrap();
    let output = tempfile::tempdir().unwrap();
    let (directory, notes) = legacy_bundle::export(&text, output.path(), Format::Json).unwrap();
    assert!(notes.contains("metadata"));
    let companion =
        bace_content_tools::parse(&std::fs::read_to_string(directory.join("native.toml")).unwrap())
            .unwrap();
    assert_eq!(companion, original);
    let converted = bace_import::import_weenie_json(
        &std::fs::read_to_string(directory.join("900003.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(converted.properties.int64s, original.properties.int64s);
    assert_eq!(converted.properties.strings, original.properties.strings);
    assert_eq!(
        converted.properties.emotes[0].actions.len(),
        original.properties.emotes[0].actions.len()
    );
}

#[test]
fn official_property_labels_and_all_editor_prototypes_are_available() {
    let labels = crate::forms::labels();
    assert!(
        labels["spell_book"]
            .iter()
            .any(|(id, name)| *id == 2 && name == "StrengthSelf1")
    );
    assert!(
        labels["strings"]
            .iter()
            .any(|(id, name)| *id == 1 && name == "Name")
    );
    assert!(
        labels["ints"]
            .iter()
            .any(|(id, name)| *id == 1 && name == "ItemType")
    );
    for section in crate::form_schema::sections().unwrap() {
        let mut doc = Document::new().unwrap();
        doc.value["properties"][section.key] = toml::Value::Array(vec![section.prototype]);
        assert!(
            doc.validated().is_ok(),
            "invalid prototype for {}",
            section.key
        );
    }
}
