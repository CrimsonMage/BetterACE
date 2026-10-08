//! Independent golden output of the unchanged pinned ACE spell factory.
use super::*;
use crate::{TreasureAssets, TreasureSpell};
use bace_content::{SparseProperties, TreasureDeathRowV1};
#[derive(Clone)]
struct Tape {
    value: f64,
    draws: usize,
}
impl TreasureRandom for Tape {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.draws += 1;
        Ok(self.value)
    }
    fn inclusive(&mut self, low: i32, high: i32) -> Result<i32, TreasureError> {
        self.draws += 1;
        Ok(low + (self.value * f64::from(high - low + 1)) as i32)
    }
}
fn profile(tier: i32, quality: f32) -> TreasureDeathRowV1 {
    TreasureDeathRowV1 {
        id: 1,
        treasure_type: 1,
        tier,
        loot_quality_mod: quality,
        unknown_chances: 0,
        item_chance: 0,
        item_min_amount: 0,
        item_max_amount: 0,
        item_treasure_type_selection_chances: 0,
        magic_item_chance: 0,
        magic_item_min_amount: 0,
        magic_item_max_amount: 0,
        magic_item_treasure_type_selection_chances: 0,
        mundane_item_chance: 0,
        mundane_item_min_amount: 0,
        mundane_item_max_amount: 0,
        mundane_item_type_selection_chances: 0,
        last_modified: String::new(),
    }
}
fn make_item(name: &str) -> (WeenieV1, TreasureRoll) {
    let mut w = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: name.to_owned(),
        weenie_type: 6,
        last_modified: None,
        properties: SparseProperties::default(),
    };
    let mut r = TreasureRoll {
        wcid: 100,
        ..Default::default()
    };
    p::si(&mut w, "WeaponSkill", en("Skill", "LightWeapons").unwrap()).unwrap();
    for (k, v) in [
        ("WieldRequirements", 7),
        ("WieldRequirements2", 7),
        ("WieldDifficulty", 100),
        ("WieldDifficulty2", 150),
    ] {
        p::si(&mut w, k, v).unwrap();
    }
    match name {
        "gem" => {
            r.item_type = 2;
            w.weenie_type = en("WeenieType", "Gem").unwrap() as u32;
        }
        "jewelry" => r.item_type = 3,
        "crown" => {
            r.item_type = 3;
            p::si(&mut w, "ArmorLevel", 10).unwrap();
        }
        "orb" | "wand" | "void" => {
            r.item_type = 9;
            r.weapon_type = 16;
            if name == "orb" {
                r.wcid = 2366;
            } else {
                p::si(
                    &mut w,
                    "DamageType",
                    en("DamageType", if name == "wand" { "Fire" } else { "Nether" }).unwrap(),
                )
                .unwrap();
            }
        }
        "melee" => {
            r.item_type = 5;
            r.weapon_type = 2;
        }
        "twohanded" => {
            r.item_type = 5;
            r.weapon_type = 18;
            p::si(
                &mut w,
                "WeaponSkill",
                en("Skill", "TwoHandedCombat").unwrap(),
            )
            .unwrap();
        }
        "missile" => {
            r.item_type = 5;
            r.weapon_type = 13;
        }
        "armor" | "shield" => {
            r.item_type = 6;
            r.armor_type = 1;
            p::si(&mut w, "ArmorLevel", 100).unwrap();
            if name == "shield" {
                p::si(&mut w, "CombatUse", en("CombatUse", "Shield").unwrap()).unwrap();
            } else {
                p::si(&mut w, "ClothingPriority", 0x400).unwrap();
            }
        }
        "shirt" => {
            r.item_type = 7;
            p::si(&mut w, "ClothingPriority", 8).unwrap();
        }
        "gloves" | "cap" => {
            r.item_type = 7;
            r.wcid = if name == "gloves" { 121 } else { 45 };
            p::si(&mut w, "ArmorLevel", 5).unwrap();
            p::si(
                &mut w,
                "ClothingPriority",
                if name == "gloves" { 0x8000 } else { 0x4000 },
            )
            .unwrap();
        }
        "dinnerware" => r.item_type = 4,
        "flask" => {
            r.item_type = 4;
            r.wcid = 7940;
        }
        _ => panic!("unknown fixture"),
    }
    r.base_armor_level = p::int(&w, "ArmorLevel").unwrap().unwrap_or(0);
    (w, r)
}
#[test]
fn complete_spell_factory_matches_compiled_official_source() {
    crate::ace_tables::install_test();
    let mut assets = TreasureAssets::default();
    for id in 1..10000 {
        assets.spells.insert(
            id,
            TreasureSpell {
                power: 0,
                base_mana: 0,
                level: 0,
                formula_level: id % 8 + 1,
            },
        );
    }
    let mut rows = 0;
    for line in include_str!("../tests/fixtures/ace_spells.tsv").lines() {
        let f: Vec<_> = line.split('|').collect();
        let (mut item, mut roll) = make_item(f[0]);
        let profile = profile(f[1].parse().unwrap(), f[3].parse().unwrap());
        let mut tape = Tape {
            value: f[2].parse().unwrap(),
            draws: 0,
        };
        let mut ctx = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut tape,
        };
        ctx.assign_spells(&mut item, &mut roll).unwrap();
        assert_eq!(tape.draws, f[4].parse::<usize>().unwrap(), "{line}");
        assert!(
            (roll.item_difficulty - f[5].parse::<f32>().unwrap()).abs() < 0.0001,
            "difficulty {line}: {}",
            roll.item_difficulty
        );
        assert_eq!(
            p::int(&item, "WieldDifficulty").unwrap(),
            Some(f[6].parse().unwrap()),
            "{line}"
        );
        assert_eq!(
            p::int(&item, "WieldDifficulty2").unwrap(),
            Some(f[7].parse().unwrap()),
            "{line}"
        );
        let expected = if f[8].is_empty() {
            vec![]
        } else {
            f[8].split(',')
                .map(|s| s.parse::<i32>().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            item.properties
                .spell_book
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            expected,
            "{line}"
        );
        rows += 1;
    }
    assert_eq!(rows, 512);
}
#[test]
fn dynamic_selection_coverage_priority_matches_source() {
    crate::ace_tables::install_test();
    let (mut w, mut roll) = make_item("armor");
    for (mask, base, armor, expected) in [
        (0x400, 100, true, 7),
        (0x4200, 100, true, 15),
        (0x8000, 100, true, 9),
        (0x4000, 21, true, 10),
        (0x10000, 21, true, 11),
        (8, 0, false, 12),
        (0x4000, 0, false, 13),
        (0x8000, 0, false, 14),
        (0x200, 100, true, 15),
        (0x10000, 0, false, 18),
        (0, 100, true, 0),
    ] {
        p::si(&mut w, "ClothingPriority", mask).unwrap();
        roll.base_armor_level = base;
        roll.armor_type = i32::from(armor);
        assert_eq!(clothing_code(&w, &roll).unwrap(), expected);
    }
}
#[test]
fn complete_magic_factory_matches_compiled_official_source() {
    crate::ace_tables::install_test();
    let mut assets = TreasureAssets::default();
    for id in 1..10000 {
        assets.spells.insert(
            id,
            TreasureSpell {
                power: id % 401,
                base_mana: id % 31 + 1,
                level: 0,
                formula_level: id % 8 + 1,
            },
        );
    }
    let mut rows = 0;
    for line in include_str!("../tests/fixtures/ace_treasure_magic.tsv").lines() {
        let f: Vec<_> = line.split('|').collect();
        let (mut item, mut roll) = make_item(f[0]);
        p::si(&mut item, "ItemWorkmanship", 5).unwrap();
        if matches!(f[0], "orb" | "wand" | "void") {
            p::sd(&mut item, "Spell", 50).unwrap();
        }
        let profile = profile(f[1].parse().unwrap(), f[3].parse().unwrap());
        let mut tape = Tape {
            value: f[2].parse().unwrap(),
            draws: 0,
        };
        let mut ctx = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut tape,
        };
        ctx.assign_magic(&mut item, &mut roll).unwrap();
        assert_eq!(tape.draws, f[4].parse::<usize>().unwrap(), "{line}");
        assert!(
            (roll.item_difficulty - f[5].parse::<f32>().unwrap()).abs() < 0.0001,
            "{line}"
        );
        for (index, name) in [
            (6, "ItemMaxMana"),
            (7, "ItemCurMana"),
            (8, "ItemSpellcraft"),
            (9, "ItemDifficulty"),
            (10, "ItemSkillLevelLimit"),
        ] {
            let expected = if f[index].is_empty() {
                None
            } else {
                Some(f[index].parse().unwrap())
            };
            assert_eq!(p::int(&item, name).unwrap(), expected, "{name}: {line}");
        }
        assert_eq!(
            p::did(&item, "ItemSkillLimit").unwrap().unwrap_or(0),
            f[11].parse::<u32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            p::float(&item, "ManaRate").unwrap().unwrap(),
            f[12].parse::<f64>().unwrap(),
            "{line}"
        );
        let expected = if f[13].is_empty() {
            vec![]
        } else {
            f[13]
                .split(',')
                .map(|s| s.parse::<i32>().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            item.properties
                .spell_book
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            expected,
            "{line}"
        );
        rows += 1;
    }
    assert_eq!(rows, 512);
}
#[test]
fn pets_and_mundane_addons_match_original_compiled_source() {
    crate::ace_tables::install_test();
    let mut assets = TreasureAssets::default();
    for id in [42516, 42517, 42518, 42635, 42636, 42637] {
        let (mut w, _) = make_item("jewelry");
        w.weenie_id = id;
        assets.templates.insert(id, std::sync::Arc::new(w));
    }
    let mut rows = 0;
    for line in include_str!("../tests/fixtures/ace_treasure_special.tsv").lines() {
        let f: Vec<_> = line.split('|').collect();
        let profile = profile(f[1].parse().unwrap(), 0.0);
        let mut tape = Tape {
            value: f[2].parse().unwrap(),
            draws: 0,
        };
        let mut ctx = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut tape,
        };
        if f[0] == "pet" {
            let (mut w, _) = make_item("jewelry");
            w.weenie_type = en("WeenieType", "PetDevice").unwrap() as u32;
            ctx.mutate_pet(&mut w).unwrap();
            for (property, expected) in [
                "GearDamage",
                "GearDamageResist",
                "GearCritDamage",
                "GearCritDamageResist",
                "GearCrit",
                "GearCritResist",
            ]
            .iter()
            .zip(f[5].split(','))
            {
                assert_eq!(
                    p::int(&w, property).unwrap(),
                    if expected.is_empty() {
                        None
                    } else {
                        Some(expected.parse().unwrap())
                    },
                    "{line}"
                );
            }
            assert_eq!(
                p::int(&w, "ItemWorkmanship").unwrap(),
                Some(f[6].parse().unwrap()),
                "{line}"
            );
        } else {
            let item = ctx.mundane_addon(f[3].parse().unwrap()).unwrap();
            if f[5].is_empty() {
                assert!(item.is_none(), "{line}");
            } else {
                let item = item.unwrap();
                assert_eq!(item.weenie_id, f[5].parse::<u32>().unwrap(), "{line}");
                assert_eq!(
                    p::int(&item, "ItemMaxLevel").unwrap(),
                    if f[6].is_empty() {
                        None
                    } else {
                        Some(f[6].parse().unwrap())
                    },
                    "{line}"
                );
                assert_eq!(
                    p::did(&item, "IconOverlay").unwrap(),
                    if f[7].is_empty() {
                        None
                    } else {
                        Some(f[7].parse().unwrap())
                    },
                    "{line}"
                );
            }
        }
        assert_eq!(tape.draws, f[4].parse::<usize>().unwrap(), "{line}");
        rows += 1;
    }
    assert_eq!(rows, 240);
}
