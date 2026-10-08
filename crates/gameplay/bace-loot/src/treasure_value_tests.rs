use super::*;
use crate::{TreasureAssets, TreasureSpell};
use bace_content::{Property, TreasureDeathRowV1};
#[derive(Clone)]
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
fn profile(tier: i32, quality: f32) -> TreasureDeathRowV1 {
    TreasureDeathRowV1 {
        id: 1,
        treasure_type: 1,
        tier,
        loot_quality_mod: quality,
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
#[test]
fn original_ace_value_and_quality_oracle() {
    crate::ace_tables::install_test();
    for line in include_str!("../tests/fixtures/value.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let mut random = Draw {
            value: f[2].parse().unwrap(),
            count: 0,
        };
        let profile = profile(f[1].parse().unwrap(), f[3].parse().unwrap());
        let mut assets = TreasureAssets::default();
        for id in [25, 8, 12] {
            assets.spells.insert(
                id,
                TreasureSpell {
                    power: 0,
                    base_mana: 0,
                    level: id % 8 + 1,
                    formula_level: 0,
                },
            );
        }
        let mut w = WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "oracle".into(),
            weenie_type: if f[0] == "gem" { 38 } else { 1 },
            last_modified: None,
            properties: Default::default(),
        };
        for (n, v) in [
            ("Value", 101),
            ("ItemWorkmanship", 7),
            ("ItemMaxMana", 55),
            ("EncumbranceVal", 123),
            ("MaterialType", en("MaterialType", "Diamond").unwrap()),
            ("GemType", en("MaterialType", "Ruby").unwrap()),
        ] {
            si(&mut w, n, v).unwrap();
        }
        if f[0] == "armor" {
            si(&mut w, "ArmorLevel", 123).unwrap();
        }
        sf(&mut w, "BulkMod", 0.63).unwrap();
        sf(&mut w, "SizeMod", 1.2).unwrap();
        sd(&mut w, "Spell", 25).unwrap();
        w.properties.spell_book = vec![
            Property { id: 8, value: 2.0 },
            Property { id: 12, value: 2.0 },
        ];
        let mut context = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut random,
        };
        context.burden(&mut w, f[0] == "weapon").unwrap();
        context.value(&mut w, &TreasureRoll::default()).unwrap();
        assert_eq!(
            (
                int(&w, "Value").unwrap().unwrap(),
                int(&w, "EncumbranceVal").unwrap().unwrap(),
                random.count
            ),
            (
                f[4].parse().unwrap(),
                f[5].parse().unwrap(),
                f[6].parse().unwrap()
            ),
            "{line}"
        );
    }
}

#[test]
fn long_description_uses_source_spell_insertion_order() {
    crate::ace_tables::install_test();
    let assets = TreasureAssets::default();
    let profile = profile(1, 0.0);
    let mut random = Draw {
        value: 0.5,
        count: 0,
    };
    let context = MutationContext {
        assets: &assets,
        profile: &profile,
        random: &mut random,
    };
    let mut w = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "oracle".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    ss(&mut w, "Name", "Ring".into()).unwrap();
    let first = en("SpellId", "EnduranceSelf1").unwrap();
    let second = en("SpellId", "StrengthOther1").unwrap();
    assert!(first > second);
    w.properties.spell_book = vec![
        Property {
            id: first,
            value: 2.0,
        },
        Property {
            id: second,
            value: 2.0,
        },
    ];
    context.long_desc(&mut w).unwrap();
    assert_eq!(
        string(&w, "LongDesc").unwrap().as_deref(),
        Some("Ring of Endurance")
    );
    sd(&mut w, "Spell", second as u32).unwrap();
    context.long_desc(&mut w).unwrap();
    assert_eq!(
        string(&w, "LongDesc").unwrap().as_deref(),
        Some("Ring of Strength")
    );
}
