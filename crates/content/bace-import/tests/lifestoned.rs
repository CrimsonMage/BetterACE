use bace_import::import_weenie_json;

#[test]
fn unchanged_community_shape_maps_all_sections_and_retains_metadata() {
    let source = include_str!("../../../../tests/fixtures/content/lifestoned-weenie.json");
    let value = import_weenie_json(source).unwrap();
    assert_eq!(value.weenie_id, 900003);
    assert_eq!(value.class_name, "ace900003-fixturenpc雪");
    let p = &value.properties;
    assert!(p.bools[0].value);
    assert_eq!(p.data_ids[0].value, u32::MAX);
    assert_eq!(p.int64s[0].value, i64::MAX);
    assert_eq!(p.secondary_attributes[0].value.current_level, 99);
    assert_eq!(p.body_parts[0].value.hlf, 0.25);
    assert_eq!(p.book_pages[1].page_text.as_deref(), Some("Second page"));
    assert!(p.book_pages[1].ignore_author);
    assert_eq!(p.create_list[0].weenie_class_id, 100);
    assert_eq!(p.skills[0].value.resistance_at_last_check, 7);
    assert_eq!(p.emotes[0].legacy_category_key, Some(1));
    assert_eq!(
        p.emotes[0].actions[0].message.as_deref(),
        Some("First action")
    );
    assert_eq!(p.emotes[0].actions[1].hero_xp64, Some(i64::MAX));
    assert_eq!(p.emotes[0].actions[1].obj_cell_id, Some(1234));
    assert_eq!(p.spell_book[0].value, 0.5);
    assert_eq!(p.positions[0].value.position_x, 1.0);
    assert_eq!(p.generators[0].legacy_slot, Some(50));
    assert_eq!(
        p.authoring_metadata.as_ref().unwrap().changelog[0]
            .comment
            .as_deref(),
        Some("initial")
    );
}

#[test]
fn upstream_known_class_name_lookup_and_optional_defaults() {
    let template = import_weenie_json(
        r#"{"wcid":1,"weenieType":10,"stringStats":[{"key":1,"value":"Name"}]}"#,
    )
    .unwrap();
    assert_eq!(template.class_name, "human");
    assert!(template.properties.authoring_metadata.is_none());
}

#[test]
fn lossy_overflow_unknown_fields_and_duplicate_properties_are_rejected() {
    for extra in [
        r#", "intStats":[{"key":1,"value":1},{"key":1,"value":2}]"#,
        r#", "createList":[{"palette":1000}]"#,
        r#", "skills":[{"key":1,"value":{"level_from_pp":65536}}]"#,
        r#", "emoteTable":[{"key":1,"value":[{"category":1,"emotes":[{"heroxp64":18446744073709551615}]}]}]"#,
        r#", "generatorTable":[{"futureField":5}]"#,
        r#", "comments":{"unexpected":"shape"}"#,
        r#", "boolStats":[{"key":1,"value":2}]"#,
    ] {
        assert!(
            import_weenie_json(&format!("{{\"wcid\":900003,\"weenieType\":1{extra}}}")).is_err(),
            "accepted {extra}"
        );
    }
}

#[test]
fn lifestoned_metadata_and_order_survive_native_binary_and_toml() {
    let source = include_str!("../../../../tests/fixtures/content/lifestoned-weenie.json");
    let value = import_weenie_json(source).unwrap();
    let binary = bace_content_tools::compile_template(&value).unwrap();
    assert_eq!(value, bace_content_tools::decode(&binary).unwrap());
    assert_eq!(
        value,
        bace_content_tools::parse(&bace_content_tools::export_binary(&binary).unwrap()).unwrap()
    );
}
