use bace_content_tools::*;
#[test]
fn editable_nested_tables_compile_into_one_supplement_and_unknown_rares_stay_disabled() {
    let graph =
        parse_loot_graph(include_str!("../../../../tests/fixtures/loot/nested.toml")).unwrap();
    let rare = parse_rare_profile(include_str!(
        "../../../../tests/fixtures/loot/rares-unconfigured.toml"
    ))
    .unwrap();
    assert!(!rare.enabled);
    assert!(rare.standard.is_none());
    assert_eq!(
        decode_loot_graph(&compile_loot_graph(&graph).unwrap()).unwrap(),
        graph
    );
    assert_eq!(
        decode_rare_profile(&compile_rare_profile(&rare).unwrap()).unwrap(),
        rare
    );
    let dir = tempfile::tempdir().unwrap();
    let built = build_loot_pack(&[graph], &[rare], dir.path()).unwrap();
    assert_eq!(built.records, 2);
    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "bace"))
            .count(),
        1
    );
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let generation = manifest.open(dir.path(), Default::default()).unwrap();
    assert!(matches!(
        generation
            .lookup(bace_storage_codec::PackKey {
                namespace: 47,
                id: 1
            })
            .unwrap(),
        bace_storage_codec::PackLookup::Record(_)
    ));
}
