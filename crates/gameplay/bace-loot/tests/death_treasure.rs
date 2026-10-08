use bace_content::{Property, TreasureDeathRowV1, WeenieV1};
use bace_loot::{
    DeathTreasure, MutationScripts, TreasureAssets, TreasureCategory, TreasureError,
    TreasureRandom, TreasureSpell, ace_tables, select_treasure,
};
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq)]
struct Draw {
    value: f64,
    count: u32,
}
impl TreasureRandom for Draw {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.count += 1;
        Ok(self.value)
    }
    fn inclusive(&mut self, min: i32, max: i32) -> Result<i32, TreasureError> {
        self.count += 1;
        Ok(min + (self.value * f64::from(max - min + 1)) as i32)
    }
}
fn profile(tier: i32, category: i32) -> TreasureDeathRowV1 {
    TreasureDeathRowV1 {
        id: 1,
        treasure_type: 1,
        tier,
        loot_quality_mod: 0.0,
        unknown_chances: 19,
        item_chance: if category == 0 { 100 } else { 0 },
        item_min_amount: 1,
        item_max_amount: 1,
        item_treasure_type_selection_chances: 1,
        magic_item_chance: if category == 1 { 100 } else { 0 },
        magic_item_min_amount: 1,
        magic_item_max_amount: 1,
        magic_item_treasure_type_selection_chances: 1,
        mundane_item_chance: 0,
        mundane_item_min_amount: 0,
        mundane_item_max_amount: 0,
        mundane_item_type_selection_chances: 1,
        last_modified: String::new(),
    }
}
fn en(k: &str, n: &str) -> i32 {
    ace_tables::enum_value(k, n).unwrap() as i32
}
fn template(id: u32, kind: u32, armor: i32, skill: i32) -> WeenieV1 {
    let mut w = WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("oracle{id}"),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    };
    for (n, v) in [
        ("Value", 100),
        ("ArmorLevel", armor),
        ("ItemType", if armor > 0 { 2 } else { 1 }),
        ("ClothingPriority", 1024),
        ("WeaponSkill", skill),
        ("Damage", 100),
        ("EncumbranceVal", 100),
        ("WeaponTime", 50),
        ("StackSize", 1),
        ("MaxStackSize", 5000),
        ("StackUnitValue", 1),
        ("StackUnitEncumbrance", 1),
    ] {
        w.properties.ints.push(Property {
            id: en("PropertyInt", n) as u32,
            value: v,
        });
    }
    w.properties.strings.push(Property {
        id: 1,
        value: "Oracle".into(),
    });
    w
}
#[test]
fn representative_death_profiles_materialize_with_prepared_assets() {
    table_support::install_tables();
    let cases = [
        (1, 0, 0.0),
        (1, 0, 0.25),
        (1, 0, 0.5),
        (1, 0, 0.9),
        (8, 0, 0.999),
        (8, 1, 0.0),
        (8, 1, 0.1),
        (8, 1, 0.25),
        (8, 1, 0.5),
        (8, 1, 0.9),
    ];
    let mut assets = TreasureAssets {
        mutation_scripts: Some(Arc::new(MutationScripts::pinned().unwrap())),
        ..Default::default()
    };
    for id in 1..=65535 {
        assets.spells.insert(
            id,
            TreasureSpell {
                power: 100,
                base_mana: 10,
                level: 3,
                formula_level: 3,
            },
        );
    }
    for &(tier, category, value) in &cases {
        let p = profile(tier, category);
        let mut random = Draw { value, count: 0 };
        let roll = select_treasure(
            &p,
            if category == 0 {
                TreasureCategory::Item
            } else {
                TreasureCategory::Magic
            },
            &mut random,
        )
        .unwrap();
        let kind = if roll.item_type == 2 {
            38
        } else if roll.is_caster() {
            35
        } else if roll.is_missile() {
            3
        } else if roll.is_melee() {
            6
        } else if roll.is_armor() {
            2
        } else {
            1
        };
        let skill = if roll.weapon_type >= 18 {
            en("Skill", "TwoHandedCombat")
        } else {
            en("Skill", "LightWeapons")
        };
        assets.templates.insert(
            roll.wcid,
            Arc::new(template(
                roll.wcid,
                kind,
                if roll.is_armor() { 100 } else { 0 },
                skill,
            )),
        );
    }
    let assets = Arc::new(assets);
    for (tier, category, value) in cases {
        let death = DeathTreasure::prepare(profile(tier, category), assets.clone(), 0.0).unwrap();
        let mut random = Draw { value, count: 0 };
        let output = death
            .generate(&mut random)
            .unwrap_or_else(|e| panic!("{tier},{category},{value}: {e:?}"));
        assert_eq!(output.len(), 1);
        assert!(random.count > 3);
        output[0].validate(Default::default()).unwrap();
    }
}
#[test]
fn missing_asset_retains_exact_rng_cursor_and_zero_partial_output() {
    table_support::install_tables();
    let mut random = Draw {
        value: 0.1,
        count: 0,
    };
    let prior = random.clone();
    let death =
        DeathTreasure::prepare(profile(1, 0), Arc::new(TreasureAssets::default()), 1.0).unwrap();
    assert!(matches!(
        death.generate(&mut random),
        Err(TreasureError::MissingTemplate(_))
    ));
    assert_eq!(random, prior);
}
#[test]
fn invalid_prepared_dependency_is_rejected_before_generation() {
    table_support::install_tables();
    let mut assets = TreasureAssets::default();
    assets.material_base.insert(
        (1, 1),
        vec![bace_loot::TreasureMaterialRow {
            material: 1,
            probability: f32::NAN,
        }],
    );
    assert!(matches!(
        DeathTreasure::prepare(profile(1, 0), Arc::new(assets), 1.0),
        Err(TreasureError::Bounds)
    ));
}
mod table_support;
