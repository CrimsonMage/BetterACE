use bace_content::{Property, TreasureWieldedRowV1, WeenieV1, WorldRecordV1};
use bace_loot::TreasureAssets;
use bace_runtime::generator_catalog::prepare_generator_catalog;
use bace_storage_codec::{PackLimits, load_manifest};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};
fn template(id: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("generator{id}"),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    }
}
fn row(id: u32, wcid: u32) -> WorldRecordV1 {
    WorldRecordV1::TreasureWielded(TreasureWieldedRowV1 {
        id,
        treasure_type: 20,
        weenie_class_id: wcid,
        palette_id: 0,
        unknown_1: 0,
        shade: 0.0,
        stack_size: 0,
        stack_size_variance: 0.0,
        probability: 1.0,
        unknown_3: 0,
        unknown_4: 0,
        unknown_5: 0,
        set_start: false,
        has_sub_set: false,
        continues_previous_set: false,
        unknown_9: 0,
        unknown_10: 0,
        unknown_11: 0,
        unknown_12: 0,
        last_modified: "2005-02-09 10:00:00".into(),
    })
}
#[test]
fn cold_catalog_joins_overloaded_ids_and_only_referenced_templates() {
    let dir = tempfile::tempdir().unwrap();
    let mut creature = template(1);
    creature
        .properties
        .data_ids
        .push(Property { id: 32, value: 20 });
    let build = bace_content_tools::build_world_pack(
        &[
            creature.clone(),
            template(100),
            template(101),
            template(999),
        ],
        &[row(12, 101), row(7, 100)],
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let generation = load_manifest(&build.manifest, PackLimits::default())
        .unwrap()
        .open(dir.path(), PackLimits::default())
        .unwrap();
    let prepared = prepare_generator_catalog(
        &generation,
        &BTreeMap::from([(1, Arc::new(creature))]),
        Arc::new(TreasureAssets::default()),
        1.0,
    )
    .unwrap();
    assert_eq!(
        prepared.templates.keys().copied().collect::<Vec<_>>(),
        [1, 100, 101]
    );
    assert_eq!(
        prepared.wielded[&20]
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        [7, 12]
    );
    assert!(prepared.treasure.is_empty());
}
#[test]
fn missing_wielded_dependency_rejects_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let mut creature = template(1);
    creature
        .properties
        .data_ids
        .push(Property { id: 32, value: 20 });
    let build = bace_content_tools::build_world_pack(
        &[creature.clone()],
        &[row(7, 100)],
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let generation = load_manifest(&build.manifest, PackLimits::default())
        .unwrap()
        .open(dir.path(), PackLimits::default())
        .unwrap();
    let error = prepare_generator_catalog(
        &generation,
        &BTreeMap::from([(1, Arc::new(creature))]),
        Arc::new(TreasureAssets::default()),
        1.0,
    )
    .err()
    .unwrap();
    assert!(
        error.contains("missing generator dependency template 100"),
        "{error}"
    );
}
#[test]
fn death_lookup_uses_treasure_type_and_does_not_prepare_unused_wielded_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let mut creature = template(1);
    creature
        .properties
        .data_ids
        .push(Property { id: 33, value: 20 });
    let death = bace_content::TreasureDeathRowV1 {
        id: 77,
        treasure_type: 20,
        tier: 1,
        loot_quality_mod: 0.0,
        unknown_chances: 1,
        item_chance: 0,
        item_min_amount: 0,
        item_max_amount: 0,
        item_treasure_type_selection_chances: 1,
        magic_item_chance: 0,
        magic_item_min_amount: 0,
        magic_item_max_amount: 0,
        magic_item_treasure_type_selection_chances: 1,
        mundane_item_chance: 0,
        mundane_item_min_amount: 0,
        mundane_item_max_amount: 0,
        mundane_item_type_selection_chances: 1,
        last_modified: "2005-02-09 10:00:00".into(),
    };
    let build = bace_content_tools::build_world_pack(
        &[creature.clone()],
        &[WorldRecordV1::TreasureDeath(death), row(7, 999)],
        dir.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let generation = load_manifest(&build.manifest, PackLimits::default())
        .unwrap()
        .open(dir.path(), PackLimits::default())
        .unwrap();
    let prepared = prepare_generator_catalog(
        &generation,
        &BTreeMap::from([(1, Arc::new(creature))]),
        Arc::new(TreasureAssets::default()),
        1.0,
    )
    .unwrap();
    assert_eq!(prepared.death[&20].id, 77);
    assert!(matches!(
        &prepared.treasure[&20],
        bace_runtime::generator_treasure::PreparedGeneratorTreasure::Death(_)
    ));
    assert_eq!(prepared.templates.len(), 1);
}
