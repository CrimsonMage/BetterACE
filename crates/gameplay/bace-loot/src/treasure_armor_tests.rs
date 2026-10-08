use super::*;
use crate::TreasureAssets;
use bace_content::TreasureDeathRowV1;
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
fn profile(tier: i32) -> TreasureDeathRowV1 {
    TreasureDeathRowV1 {
        id: 1,
        treasure_type: 1,
        tier,
        loot_quality_mod: 0.2,
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
fn original_armor_routing_resistance_gear_and_equipment_sets() {
    crate::ace_tables::install_test();
    for line in include_str!("../tests/fixtures/armor.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let scenario: i32 = f[0].parse().unwrap();
        let mut random = Draw {
            value: f[2].parse().unwrap(),
            count: 0,
        };
        let profile = profile(f[1].parse().unwrap());
        let assets = TreasureAssets::default();
        let mut w = WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "oracle".into(),
            weenie_type: 2,
            last_modified: None,
            properties: Default::default(),
        };
        let mut treasure = TreasureRoll {
            item_type: 6,
            armor_type: 1,
            ..Default::default()
        };
        for (n, v) in [
            ("ArmorLevel", 100),
            ("ClothingPriority", 1024),
            (
                "WieldRequirements",
                en("WieldRequirement", "Level").unwrap(),
            ),
            ("WieldDifficulty", 150),
        ] {
            si(&mut w, n, v).unwrap();
        }
        sf(&mut w, "ArmorModVsFire", 1.0).unwrap();
        if matches!(scenario, 1 | 2) {
            si(&mut w, "CombatUse", en("CombatUse", "Shield").unwrap()).unwrap();
        }
        match scenario {
            2 => treasure.armor_type = 11,
            3 => treasure.armor_type = 19,
            4 => si(
                &mut w,
                "ClothingPriority",
                en("CoverageMask", "Head").unwrap(),
            )
            .unwrap(),
            5 | 6 => {
                si(&mut w, "ArmorLevel", 0).unwrap();
                treasure.item_type = if scenario == 5 { 3 } else { 7 };
                treasure.armor_type = 0;
            }
            7 => {
                treasure.armor_type = 25;
                si(
                    &mut w,
                    "WieldRequirements",
                    en("WieldRequirement", "IntStat").unwrap(),
                )
                .unwrap();
            }
            8 => si(
                &mut w,
                "WieldRequirements",
                en("WieldRequirement", "RawSkill").unwrap(),
            )
            .unwrap(),
            _ => {}
        }
        assert_eq!(
            format!("ArmorLevel.{}.txt", armor_script(&w, &treasure).unwrap()),
            f[3],
            "{line}"
        );
        let mut ctx = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut random,
        };
        ctx.armor_resistance(&mut w, "ArmorModVsFire").unwrap();
        ctx.gear_rating(&mut w, &treasure).unwrap();
        let set = ctx.equipment_set(&w).unwrap().unwrap_or(0);
        assert_eq!(
            float(&w, "ArmorModVsFire").unwrap().unwrap(),
            f[4].parse::<f64>().unwrap(),
            "{line}"
        );
        for (n, v) in [
            "GearCritDamage",
            "GearCritDamageResist",
            "GearDamage",
            "GearDamageResist",
            "GearHealingBoost",
            "GearMaxHealth",
        ]
        .into_iter()
        .zip(f[5].split(','))
        {
            assert_eq!(
                int(&w, n).unwrap().unwrap_or(0),
                v.parse::<i32>().unwrap(),
                "{n}:{line}"
            );
        }
        for (n, v) in [
            "WieldRequirements",
            "WieldDifficulty",
            "WieldRequirements2",
            "WieldDifficulty2",
        ]
        .into_iter()
        .zip(f[6].split(','))
        {
            assert_eq!(
                int(&w, n).unwrap().unwrap_or(0),
                v.parse::<i32>().unwrap(),
                "{n}:{line}"
            );
        }
        assert_eq!(
            (set, random.count),
            (f[7].parse().unwrap(), f[8].parse().unwrap()),
            "{line}"
        );
    }
}
