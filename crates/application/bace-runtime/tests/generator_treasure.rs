use bace_content::{TreasureDeathRowV1, TreasureWieldedRowV1, WeenieV1};
use bace_loot::{TreasureAssets, TreasureCategory, TreasureError, TreasureRandom, select_treasure};
use bace_runtime::generator_treasure::PreparedGeneratorTreasure;
use std::sync::Arc;
mod treasure_table_support;
#[derive(Clone, PartialEq, Debug)]
struct Draw(u32, f64);
impl TreasureRandom for Draw {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.0 += 1;
        Ok(self.1)
    }
    fn inclusive(&mut self, min: i32, _max: i32) -> Result<i32, TreasureError> {
        self.0 += 1;
        Ok(min)
    }
}
fn death() -> TreasureDeathRowV1 {
    TreasureDeathRowV1 {
        id: 1,
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
        last_modified: String::new(),
    }
}
fn wielded() -> TreasureWieldedRowV1 {
    TreasureWieldedRowV1 {
        id: 1,
        treasure_type: 20,
        weenie_class_id: 100,
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
        last_modified: String::new(),
    }
}
#[test]
fn death_precedes_wielded_even_when_death_outputs_nothing() {
    let prepared = PreparedGeneratorTreasure::prepare(
        20,
        Some(death()),
        vec![wielded()],
        Arc::new(TreasureAssets::default()),
        1.0,
    )
    .unwrap();
    let mut rng = Draw(0, 0.5);
    assert!(prepared.generate(&mut rng).unwrap().is_empty());
    assert_eq!(rng.0, 3);
}
#[test]
fn death_asset_failure_never_falls_back_or_advances_rng() {
    treasure_table_support::install_tables();
    let mut p = death();
    p.item_chance = 100;
    p.item_min_amount = 1;
    p.item_max_amount = 1;
    let mut assets = TreasureAssets::default();
    assets.templates.insert(
        100,
        Arc::new(WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "fallback".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        }),
    );
    // The pinned tier-one Item profile selects a real WCID on this draw.
    // A 0.5 draw selects a no-item branch and never reaches the missing asset.
    assert_ne!(
        select_treasure(&p, TreasureCategory::Item, &mut Draw(0, 0.1))
            .unwrap()
            .wcid,
        0
    );
    let prepared =
        PreparedGeneratorTreasure::prepare(20, Some(p), vec![wielded()], Arc::new(assets), 1.0)
            .unwrap();
    let mut rng = Draw(0, 0.1);
    assert!(matches!(
        prepared.generate(&mut rng),
        Err(TreasureError::MissingTemplate(_))
    ));
    assert_eq!(rng, Draw(0, 0.1));
}
#[test]
fn wielded_requires_matching_table_and_prepared_source() {
    let mut assets = TreasureAssets::default();
    assets.templates.insert(
        100,
        Arc::new(WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "source".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        }),
    );
    let assets = Arc::new(assets);
    assert!(matches!(
        PreparedGeneratorTreasure::prepare(20, None, vec![], assets.clone(), 1.0),
        Err(TreasureError::MissingTable(20))
    ));
    assert!(matches!(
        PreparedGeneratorTreasure::prepare(21, None, vec![wielded()], assets.clone(), 1.0),
        Err(TreasureError::Bounds)
    ));
    let prepared =
        PreparedGeneratorTreasure::prepare(20, None, vec![wielded()], assets, 1.0).unwrap();
    assert_eq!(
        prepared.generate(&mut Draw(0, 0.5)).unwrap()[0].weenie_id,
        100
    );
}
