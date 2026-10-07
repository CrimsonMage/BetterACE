use bace_content::{Emote, EmoteAction, Property};
use bace_content_tools::{compile, compile_template, decode, export_binary, parse};

const SOURCE: &str = r#"
schema_version = 1
weenie_id = 50
class_name = "test-template"
weenie_type = 1
[properties]
ints = [{ id = 90000, value = -2147483648 }, { id = 1, value = 2147483647 }]
int64s = [{ id = 1, value = -9223372036854775808 }]
strings = [{ id = 1, value = "Unicode: 雪 🦀" }]
"#;

#[test]
fn canonical_dictionary_order_unknown_ids_and_unicode_survive() {
    let first = compile(SOURCE).unwrap();
    let mut template = parse(SOURCE).unwrap();
    template.properties.ints.reverse();
    assert_eq!(first, compile_template(&template).unwrap());
    let native = export_binary(&first).unwrap();
    assert_eq!(first, compile(&native).unwrap());
    let decoded = decode(&first).unwrap();
    assert_eq!(decoded.properties.ints[1].id, 90000);
    assert_eq!(decoded.properties.int64s[0].value, i64::MIN);
    assert!(decoded.properties.strings[0].value.contains('雪'));
}

#[test]
fn authored_emote_action_order_and_optional_values_survive() {
    let mut template = parse(SOURCE).unwrap();
    template.properties.emotes.push(Emote {
        actions: vec![
            EmoteAction {
                r#type: 50,
                amount64: Some(i64::MAX),
                message: Some("first".into()),
                ..Default::default()
            },
            EmoteAction {
                r#type: 1,
                message: Some("second".into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    template.properties.bools.push(Property {
        id: 123,
        value: false,
    });
    let bytes = compile_template(&template).unwrap();
    assert_eq!(template, decode(&bytes).unwrap());
    assert_eq!(template, parse(&export_binary(&bytes).unwrap()).unwrap());
}

#[test]
fn invalid_toml_unknown_fields_duplicates_and_nonfinite_fail() {
    assert!(compile(&format!("{SOURCE}\nunknown_field = 1")).is_err());
    assert!(compile(&SOURCE.replace("id = 90000", "id = 1")).is_err());
    assert!(compile(&format!("{SOURCE}\nfloats = [{{id=1,value=nan}}]")).is_err());
    assert!(compile(&SOURCE.replace("schema_version = 1", "schema_version = 2")).is_err());
}

#[test]
fn every_upstream_property_family_survives_toml_binary_export() {
    let source = include_str!("../../../../tests/fixtures/content/all-families.toml");
    let template = parse(source).unwrap();
    let bytes = compile(source).unwrap();
    assert_eq!(template, decode(&bytes).unwrap());
    assert_eq!(template, parse(&export_binary(&bytes).unwrap()).unwrap());
    assert_eq!(template.properties.body_parts[0].id, -1);
    assert_eq!(template.properties.spell_book[0].id, -1);
    assert_eq!(
        template.properties.emotes[0].actions[0].max64,
        Some(i64::MAX)
    );
    assert!(template.properties.floats[0].value.is_sign_negative());
}

#[test]
fn oversized_collection_is_rejected_during_binary_decode() {
    let mut value = parse(SOURCE).unwrap();
    value.properties.event_filter = (0..100_001).collect();
    // Bypass normal compilation to model an independently authored DB payload.
    let bytes =
        bace_storage_codec::encode(1, 1, &value, bace_storage_codec::CodecLimits::default())
            .unwrap();
    assert!(matches!(
        decode(&bytes),
        Err(bace_content_tools::ToolError::Codec(_))
    ));
}
