use bace_import::import_weenie_json;

#[test]
fn upstream_names_numeric_unknown_ids_and_order_are_preserved() {
    let source = r#"{
      "WeenieClassId": 42, "ClassName": "fixture", "WeenieType": "Generic",
      "PropertiesInt": {"ItemType": 1, "90000": -2147483648},
      "PropertiesInt64": {"TotalExperience": 9223372036854775807},
      "PropertiesString": {"Name": "Unicode 雪"},
      "PropertiesEmote": [{"Category": 1, "Probability": 1.0,
        "PropertiesEmoteAction": [{"Type": 50, "Message": "first"}, {"Type": 1, "Message": "second"}]}],
      "PropertiesGenerator": [{"WeenieClassId": 500, "Probability": 0.5, "Delay": null}],
      "PropertiesPosition": {"Location": {"ObjCellId": 1234, "RotationW": 1.0}},
      "PropertiesBool": null
    }"#;
    let value = import_weenie_json(source).unwrap();
    assert_eq!(value.properties.ints[0].id, 1);
    assert_eq!(value.properties.ints[1].id, 90000);
    assert_eq!(value.properties.int64s[0].value, i64::MAX);
    assert_eq!(value.properties.emotes[0].actions[0].r#type, 50);
    assert_eq!(value.properties.emotes[0].actions[1].r#type, 1);
    assert_eq!(value.properties.generators[0].delay, None);
    assert_eq!(value.properties.positions[0].value.rotation_w, 1.0);
}

#[test]
fn unsupported_and_duplicate_fields_are_errors_not_silent_loss() {
    let prefix = r#""WeenieClassId":42,"ClassName":"fixture","WeenieType":1"#;
    for extra in [
        r#", "FutureProperty": 1"#,
        r#", "PropertiesInt": {"1": 1, "1": 2}"#,
        r#", "PropertiesInt": {"1": 1, "ItemType": 2}"#,
        r#", "PropertiesInt": {"FutureName": 1}"#,
        r#", "PropertiesEmote": [{"Object": {"ClassName":"embedded"}}]"#,
    ] {
        assert!(
            import_weenie_json(&format!("{{{prefix}{extra}}}")).is_err(),
            "accepted {extra}"
        );
    }
}

#[test]
fn signed_body_spell_and_skill_ids_are_not_coerced_to_unsigned() {
    let source = r#"{"WeenieClassId":42,"ClassName":"signed","WeenieType":1,
        "PropertiesBodyPart":{"Undefined":{"DType":-1}},
        "PropertiesSkill":{"-1":{}},"PropertiesSpellBook":{"-1":0.5}}"#;
    let value = import_weenie_json(source).unwrap();
    assert_eq!(value.properties.body_parts[0].id, -1);
    assert_eq!(value.properties.body_parts[0].value.d_type, -1);
    assert_eq!(value.properties.skills[0].id, -1);
    assert_eq!(value.properties.spell_book[0].id, -1);
}
