use super::*;
use crate::{TreasureAssets, TreasureColorRow, TreasureMaterialRow};
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
#[test]
fn original_material_and_dat_palette_intersection_oracle() {
    crate::ace_tables::install_test();
    for line in include_str!("../tests/fixtures/materials.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let scenario: u32 = f[0].parse().unwrap();
        let tier: i32 = f[1].parse().unwrap();
        let mut random = Draw {
            value: f[2].parse().unwrap(),
            count: 0,
        };
        let mut assets = TreasureAssets::default();
        if scenario != 2 {
            assets.material_base.insert(
                (1, tier.min(6) as u32),
                vec![
                    TreasureMaterialRow {
                        material: en("MaterialType", "Ivory").unwrap() as u32,
                        probability: 0.2,
                    },
                    TreasureMaterialRow {
                        material: 1,
                        probability: 0.8,
                    },
                ],
            );
        }
        if scenario != 3 {
            assets.material_group.insert(
                (1, tier.min(6) as u32),
                vec![
                    TreasureMaterialRow {
                        material: en("MaterialType", "Gold").unwrap() as u32,
                        probability: 0.3,
                    },
                    TreasureMaterialRow {
                        material: en("MaterialType", "Silver").unwrap() as u32,
                        probability: 0.7,
                    },
                ],
            );
        }
        if !matches!(scenario, 4 | 5) {
            for n in [
                "Copper", "Bronze", "Iron", "Steel", "Silver", "Gold", "Ivory",
            ] {
                assets.material_colors.insert(
                    (en("MaterialType", n).unwrap() as u32, 0),
                    (1..=10)
                        .map(|palette| TreasureColorRow {
                            palette,
                            probability: 0.1,
                        })
                        .collect(),
                );
            }
        }
        assets.clothing_palettes.insert(
            7,
            if scenario == 6 {
                Default::default()
            } else {
                (1..=10)
                    .map(|i| (i, if scenario == 7 { 0 } else { 100 + i }))
                    .collect()
            },
        );
        let profile = profile(tier);
        let mut w = WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "oracle".into(),
            weenie_type: en("WeenieType", "Clothing").unwrap() as u32,
            last_modified: None,
            properties: Default::default(),
        };
        si(&mut w, "ItemType", en("ItemType", "Armor").unwrap()).unwrap();
        if scenario != 1 {
            si(
                &mut w,
                "TsysMutationData",
                if scenario == 5 { 1 | (1 << 16) } else { 1 },
            )
            .unwrap();
        }
        sd(&mut w, "ClothingBase", 7).unwrap();
        let mut ctx = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut random,
        };
        let material = ctx.material(&w).unwrap();
        si(&mut w, "MaterialType", material).unwrap();
        ctx.color(&mut w).unwrap();
        assert_eq!(
            (
                material,
                did(&w, "Icon").unwrap().unwrap_or(0),
                int(&w, "PaletteTemplate").unwrap().unwrap_or(0),
                float(&w, "Shade").unwrap().unwrap_or(0.0),
                random.count
            ),
            (
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
                f[5].parse().unwrap(),
                f[6].parse().unwrap(),
                f[7].parse().unwrap()
            ),
            "{line}"
        );
    }
}
#[test]
fn original_cloak_mutation_includes_value_and_level_requirements() {
    crate::ace_tables::install_test();
    for line in include_str!("../tests/fixtures/cloak.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let mut profile = profile(f[0].parse().unwrap());
        profile.loot_quality_mod = 0.2;
        let assets = TreasureAssets::default();
        let mut random = Draw {
            value: f[1].parse().unwrap(),
            count: 0,
        };
        let mut w = WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "cloak".into(),
            weenie_type: en("WeenieType", "Clothing").unwrap() as u32,
            last_modified: None,
            properties: Default::default(),
        };
        for (n, v) in [
            ("ItemType", en("ItemType", "Clothing").unwrap()),
            ("Value", 100),
            ("NumItemsInMaterial", 2),
        ] {
            si(&mut w, n, v).unwrap();
        }
        let mut ctx = MutationContext {
            assets: &assets,
            profile: &profile,
            random: &mut random,
        };
        ctx.mutate_cloak(
            &mut w,
            &crate::TreasureRoll {
                item_type: 25,
                ..Default::default()
            },
        )
        .unwrap();
        for (n, v) in [
            "ItemMaxLevel",
            "WieldDifficulty",
            "EquipmentSetId",
            "CloakWeaveProc",
            "MaterialType",
            "ItemWorkmanship",
            "Value",
            "GearDamage",
            "GearDamageResist",
            "WieldRequirements",
        ]
        .into_iter()
        .zip(f[2].split(','))
        {
            assert_eq!(
                int(&w, n).unwrap().unwrap_or(0),
                v.parse::<i32>().unwrap(),
                "{n}:{line}"
            );
        }
        for (n, v) in ["IconOverlay", "ProcSpell"]
            .into_iter()
            .zip(f[3].split(','))
        {
            assert_eq!(
                did(&w, n).unwrap().unwrap_or(0),
                v.parse::<u32>().unwrap(),
                "{n}:{line}"
            );
        }
        let id = id("PropertyBool", "ProcSpellSelfTargeted").unwrap();
        assert_eq!(
            w.properties
                .bools
                .iter()
                .find(|p| p.id == id)
                .is_some_and(|p| p.value),
            f[4] == "True",
            "{line}"
        );
        assert_eq!(random.count, f[5].parse::<u32>().unwrap(), "{line}");
    }
}
