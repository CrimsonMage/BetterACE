use crate::recipe_workspace::find_links;
use bace_content::{
    CookBookRowV1, RecipeModRowV1, RecipeModsIntRowV1, RecipeRequirementsIntRowV1, RecipeRowV1,
    WorldRecordV1,
};
use bace_storage_codec::{PackKey, PackLimits, PackLookup, load_manifest};
use std::fs;
use std::sync::atomic::AtomicBool;

#[test]
fn recipe_view_groups_main_use_requirement_effect_and_saved_draft() {
    let root = tempfile::tempdir().unwrap();
    let base = root.path().join("base");
    let drafts = root.path().join("drafts");
    fs::create_dir_all(&base).unwrap();
    fs::create_dir_all(drafts.join("world")).unwrap();
    let weenie = bace_content_tools::parse(
        "schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=10\n[properties]\n",
    )
    .unwrap();
    let recipe = WorldRecordV1::Recipe(RecipeRowV1 {
        id: 7,
        unknown_1: 0,
        skill: 1,
        difficulty: 10,
        salvage_type: 0,
        success_w_c_i_d: 42,
        success_amount: 1,
        success_message: None,
        fail_w_c_i_d: 0,
        fail_amount: 0,
        fail_message: None,
        success_destroy_source_chance: 0.0,
        success_destroy_source_amount: 0,
        success_destroy_source_message: None,
        success_destroy_target_chance: 0.0,
        success_destroy_target_amount: 0,
        success_destroy_target_message: None,
        fail_destroy_source_chance: 0.0,
        fail_destroy_source_amount: 0,
        fail_destroy_source_message: None,
        fail_destroy_target_chance: 0.0,
        fail_destroy_target_amount: 0,
        fail_destroy_target_message: None,
        data_id: 0,
        last_modified: "2026-10-08 00:00:00".into(),
    });
    let use_row = WorldRecordV1::CookBook(CookBookRowV1 {
        id: 2,
        recipe_id: 7,
        source_w_c_i_d: 10,
        target_w_c_i_d: 11,
        last_modified: "2026-10-08 00:00:00".into(),
    });
    let effect = WorldRecordV1::RecipeMod(RecipeModRowV1 {
        id: 3,
        recipe_id: 7,
        executes_on_success: true,
        health: 0,
        stamina: 0,
        mana: 0,
        unknown_7: false,
        data_id: 0,
        unknown_9: 0,
        instance_id: 0,
    });
    let detail = WorldRecordV1::RecipeModsInt(RecipeModsIntRowV1 {
        id: 4,
        recipe_mod_id: 3,
        index: 0,
        stat: 1,
        value: 2,
        r#enum: 0,
        source: 0,
    });
    let requirement = WorldRecordV1::RecipeRequirementsInt(RecipeRequirementsIntRowV1 {
        id: 5,
        recipe_id: 7,
        index: 0,
        stat: 1,
        value: 10,
        r#enum: 0,
        message: None,
    });
    let built = bace_content_tools::build_world_pack(
        &[weenie],
        &[recipe, use_row, effect, detail, requirement.clone()],
        &base,
        &AtomicBool::new(false),
    )
    .unwrap();
    let links = find_links(&built.manifest, Some(&drafts), 7).unwrap();
    assert_eq!(links.len(), 5);
    for (namespace, id) in [(24, 7), (16, 2), (25, 3), (30, 4), (36, 5)] {
        assert!(
            links
                .iter()
                .any(|link| link.key == PackKey { namespace, id })
        );
    }
    let WorldRecordV1::RecipeRequirementsInt(mut changed) = requirement else {
        panic!("test requirement variant")
    };
    changed.value = 20;
    let path = drafts.join("world/custom-requirement.toml");
    fs::write(
        &path,
        bace_content_tools::export_world_record(&WorldRecordV1::RecipeRequirementsInt(changed))
            .unwrap(),
    )
    .unwrap();
    let links = find_links(&built.manifest, Some(&drafts), 7).unwrap();
    assert_eq!(links.len(), 5);
    assert_eq!(
        links
            .iter()
            .find(|link| link.key.namespace == 36)
            .unwrap()
            .draft_path
            .as_deref(),
        Some(path.as_path())
    );
}

#[test]
#[ignore = "Requires BACE_WORLD_MANIFEST pointing to a complete local 16PY pack"]
fn supplied_world_recipe_links_scan_is_bounded_and_reopenable() {
    let manifest_path = std::env::var_os("BACE_WORLD_MANIFEST")
        .map(std::path::PathBuf::from)
        .expect("Set BACE_WORLD_MANIFEST");
    let limits = PackLimits::default();
    let generation = load_manifest(&manifest_path, limits)
        .unwrap()
        .open(manifest_path.parent().unwrap(), limits)
        .unwrap();
    let id = generation
        .scan(
            Some(PackKey {
                namespace: 23,
                id: u64::MAX,
            }),
            64,
        )
        .unwrap()
        .into_iter()
        .find_map(|(key, row)| {
            (key.namespace == 24 && matches!(row, PackLookup::Record(_)))
                .then_some(u32::try_from(key.id).unwrap())
        })
        .expect("No recipe row in the first recipe page");
    let links = find_links(&manifest_path, None, id).unwrap();
    assert!(links.iter().any(|link| link.key
        == PackKey {
            namespace: 24,
            id: u64::from(id)
        }));
}
