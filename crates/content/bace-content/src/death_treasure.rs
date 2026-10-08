//! Frozen authoring DTO for pinned ACE.Database.Models.World.TreasureDeath.
//! Source: ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b, AGPL-3.0-only.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathTreasureV1 {
    pub schema_version: u16,
    pub treasure_type: u32,
    pub tier: i32,
    pub loot_quality_mod: f32,
    pub unknown_chances: i32,
    pub item_chance: i32,
    pub item_min_amount: i32,
    pub item_max_amount: i32,
    pub item_treasure_type_selection_chances: i32,
    pub magic_item_chance: i32,
    pub magic_item_min_amount: i32,
    pub magic_item_max_amount: i32,
    pub magic_item_treasure_type_selection_chances: i32,
    pub mundane_item_chance: i32,
    pub mundane_item_min_amount: i32,
    pub mundane_item_max_amount: i32,
    pub mundane_item_type_selection_chances: i32,
}
impl Default for DeathTreasureV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            treasure_type: 1,
            tier: 1,
            loot_quality_mod: 0.0,
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
        }
    }
}
impl DeathTreasureV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported death-treasure schema".into());
        }
        if self.treasure_type == 0 {
            return Err("Treasure type must be nonzero".into());
        }
        if !(1..=8).contains(&self.tier) {
            return Err("Loot tier must be between 1 and 8".into());
        }
        if !self.loot_quality_mod.is_finite() {
            return Err("Loot quality must be finite".into());
        }
        for (name, chance, min, max) in [
            (
                "Items",
                self.item_chance,
                self.item_min_amount,
                self.item_max_amount,
            ),
            (
                "Magic items",
                self.magic_item_chance,
                self.magic_item_min_amount,
                self.magic_item_max_amount,
            ),
            (
                "Mundane items",
                self.mundane_item_chance,
                self.mundane_item_min_amount,
                self.mundane_item_max_amount,
            ),
        ] {
            if !(0..=100).contains(&chance) || min < 0 || min > max {
                return Err(format!(
                    "{name}: chance must be 0–100, with 0 <= minimum <= maximum"
                ));
            }
        }
        Ok(())
    }
}
