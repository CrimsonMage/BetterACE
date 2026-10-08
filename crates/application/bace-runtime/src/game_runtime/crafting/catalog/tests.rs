use super::Catalog;
use bace_content::*;
use std::sync::atomic::AtomicBool;
fn source() -> RecipeRowV1 {
    RecipeRowV1 {
        id: 1,
        unknown_1: 0,
        skill: 28,
        difficulty: 100,
        salvage_type: 1,
        success_w_c_i_d: 0,
        success_amount: 0,
        success_message: Some("success".into()),
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
        last_modified: String::new(),
    }
}

fn item(id: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("craft_{id}"),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    }
}
#[test]
fn accepted_base_pack_joins_exact_recipe_rows_with_nonzero_domain_revision() {
    let source_item = item(20);
    let target = item(30);
    let unrelated = item(40);
    let rows = vec![
        WorldRecordV1::CookBook(CookBookRowV1 {
            last_modified: String::new(),
            id: 1,
            recipe_id: 1,
            source_w_c_i_d: 20,
            target_w_c_i_d: 30,
        }),
        WorldRecordV1::Recipe(source()),
        WorldRecordV1::RecipeMod(RecipeModRowV1 {
            id: 7,
            recipe_id: 1,
            executes_on_success: true,
            health: 0,
            stamina: 0,
            mana: 0,
            unknown_7: false,
            data_id: 0x38000011,
            unknown_9: 0,
            instance_id: 0,
        }),
    ];
    let directory = tempfile::tempdir().unwrap();
    let built = bace_content_tools::build_world_pack(
        &[source_item.clone(), target.clone(), unrelated.clone()],
        &rows,
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let generation = manifest.open(directory.path(), Default::default()).unwrap();
    let expected_revision = generation.revision().checked_add(1).unwrap();
    let catalog = Catalog::load(&generation).unwrap();
    let prepared = catalog.prepare(&source_item, &target).unwrap();
    assert_eq!(prepared.recipe.revision, expected_revision);
    assert_eq!(prepared.recipe.success.mutations.len(), 1);
    assert!(matches!(
        prepared.recipe.success.mutations[0].kind,
        bace_crafting::MutationKind::Script(0x38000011)
    ));
    assert!(catalog.prepare(&source_item, &unrelated).is_err());
    assert!(catalog.matches_generation(&generation));
    let mut changed = source();
    changed.difficulty = 200;
    let delta = bace_storage_codec::compile_pack(
        directory.path(),
        [Ok(bace_storage_codec::PackRecord {
            key: bace_storage_codec::PackKey {
                namespace: 24,
                id: 1,
            },
            schema: 1,
            value: Some(
                bace_content_tools::compile_world_record(&WorldRecordV1::Recipe(changed)).unwrap(),
            ),
        })],
        Default::default(),
    )
    .unwrap();
    let mut next = manifest;
    next.generation += 1;
    next.deltas.push(delta);
    let updated = next.open(directory.path(), Default::default()).unwrap();
    assert!(!catalog.matches_generation(&updated));
    let new_catalog = Catalog::load(&updated).unwrap();
    assert_eq!(prepared.recipe.proficiency, Some((28, 100)));
    assert_eq!(
        new_catalog
            .prepare(&source_item, &target)
            .unwrap()
            .recipe
            .proficiency,
        Some((28, 200))
    );
}
