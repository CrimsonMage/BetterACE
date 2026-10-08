//! Frozen world rows from official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! AGPL-3.0-only. Generated from data/world-base.sql; field order is schema 1.
use crate::world_rows::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorldRecordV1 {
    CookBook(CookBookRowV1),
    Encounter(EncounterRowV1),
    Event(EventRowV1),
    HousePortal(HousePortalRowV1),
    LandblockInstance(LandblockInstanceRowV1),
    LandblockInstanceLink(LandblockInstanceLinkRowV1),
    PointsOfInterest(PointsOfInterestRowV1),
    Quest(QuestRowV1),
    Recipe(RecipeRowV1),
    RecipeMod(RecipeModRowV1),
    RecipeModsBool(RecipeModsBoolRowV1),
    RecipeModsDID(RecipeModsDIDRowV1),
    RecipeModsFloat(RecipeModsFloatRowV1),
    RecipeModsIID(RecipeModsIIDRowV1),
    RecipeModsInt(RecipeModsIntRowV1),
    RecipeModsString(RecipeModsStringRowV1),
    RecipeRequirementsBool(RecipeRequirementsBoolRowV1),
    RecipeRequirementsDID(RecipeRequirementsDIDRowV1),
    RecipeRequirementsFloat(RecipeRequirementsFloatRowV1),
    RecipeRequirementsIID(RecipeRequirementsIIDRowV1),
    RecipeRequirementsInt(RecipeRequirementsIntRowV1),
    RecipeRequirementsString(RecipeRequirementsStringRowV1),
    Spell(Box<SpellRowV1>),
    TreasureDeath(TreasureDeathRowV1),
    TreasureGemCount(TreasureGemCountRowV1),
    TreasureMaterialBase(TreasureMaterialBaseRowV1),
    TreasureMaterialColor(TreasureMaterialColorRowV1),
    TreasureMaterialGroups(TreasureMaterialGroupsRowV1),
    TreasureWielded(TreasureWieldedRowV1),
    Version(VersionRowV1),
}
impl WorldRecordV1 {
    pub fn table_name(&self) -> &'static str {
        match self {
            Self::CookBook(_) => "cook_book",
            Self::Encounter(_) => "encounter",
            Self::Event(_) => "event",
            Self::HousePortal(_) => "house_portal",
            Self::LandblockInstance(_) => "landblock_instance",
            Self::LandblockInstanceLink(_) => "landblock_instance_link",
            Self::PointsOfInterest(_) => "points_of_interest",
            Self::Quest(_) => "quest",
            Self::Recipe(_) => "recipe",
            Self::RecipeMod(_) => "recipe_mod",
            Self::RecipeModsBool(_) => "recipe_mods_bool",
            Self::RecipeModsDID(_) => "recipe_mods_d_i_d",
            Self::RecipeModsFloat(_) => "recipe_mods_float",
            Self::RecipeModsIID(_) => "recipe_mods_i_i_d",
            Self::RecipeModsInt(_) => "recipe_mods_int",
            Self::RecipeModsString(_) => "recipe_mods_string",
            Self::RecipeRequirementsBool(_) => "recipe_requirements_bool",
            Self::RecipeRequirementsDID(_) => "recipe_requirements_d_i_d",
            Self::RecipeRequirementsFloat(_) => "recipe_requirements_float",
            Self::RecipeRequirementsIID(_) => "recipe_requirements_i_i_d",
            Self::RecipeRequirementsInt(_) => "recipe_requirements_int",
            Self::RecipeRequirementsString(_) => "recipe_requirements_string",
            Self::Spell(_) => "spell",
            Self::TreasureDeath(_) => "treasure_death",
            Self::TreasureGemCount(_) => "treasure_gem_count",
            Self::TreasureMaterialBase(_) => "treasure_material_base",
            Self::TreasureMaterialColor(_) => "treasure_material_color",
            Self::TreasureMaterialGroups(_) => "treasure_material_groups",
            Self::TreasureWielded(_) => "treasure_wielded",
            Self::Version(_) => "version",
        }
    }
    pub fn namespace(&self) -> u16 {
        match self {
            Self::CookBook(_) => 16,
            Self::Encounter(_) => 17,
            Self::Event(_) => 18,
            Self::HousePortal(_) => 19,
            Self::LandblockInstance(_) => 20,
            Self::LandblockInstanceLink(_) => 21,
            Self::PointsOfInterest(_) => 22,
            Self::Quest(_) => 23,
            Self::Recipe(_) => 24,
            Self::RecipeMod(_) => 25,
            Self::RecipeModsBool(_) => 26,
            Self::RecipeModsDID(_) => 27,
            Self::RecipeModsFloat(_) => 28,
            Self::RecipeModsIID(_) => 29,
            Self::RecipeModsInt(_) => 30,
            Self::RecipeModsString(_) => 31,
            Self::RecipeRequirementsBool(_) => 32,
            Self::RecipeRequirementsDID(_) => 33,
            Self::RecipeRequirementsFloat(_) => 34,
            Self::RecipeRequirementsIID(_) => 35,
            Self::RecipeRequirementsInt(_) => 36,
            Self::RecipeRequirementsString(_) => 37,
            Self::Spell(_) => 38,
            Self::TreasureDeath(_) => 39,
            Self::TreasureGemCount(_) => 40,
            Self::TreasureMaterialBase(_) => 41,
            Self::TreasureMaterialColor(_) => 42,
            Self::TreasureMaterialGroups(_) => 43,
            Self::TreasureWielded(_) => 44,
            Self::Version(_) => 45,
        }
    }
    pub fn id(&self) -> u64 {
        match self {
            Self::CookBook(r) => u64::from(r.id),
            Self::Encounter(r) => u64::from(r.id),
            Self::Event(r) => u64::from(r.id),
            Self::HousePortal(r) => u64::from(r.id),
            Self::LandblockInstance(r) => u64::from(r.guid),
            Self::LandblockInstanceLink(r) => u64::from(r.id),
            Self::PointsOfInterest(r) => u64::from(r.id),
            Self::Quest(r) => u64::from(r.id),
            Self::Recipe(r) => u64::from(r.id),
            Self::RecipeMod(r) => u64::from(r.id),
            Self::RecipeModsBool(r) => u64::from(r.id),
            Self::RecipeModsDID(r) => u64::from(r.id),
            Self::RecipeModsFloat(r) => u64::from(r.id),
            Self::RecipeModsIID(r) => u64::from(r.id),
            Self::RecipeModsInt(r) => u64::from(r.id),
            Self::RecipeModsString(r) => u64::from(r.id),
            Self::RecipeRequirementsBool(r) => u64::from(r.id),
            Self::RecipeRequirementsDID(r) => u64::from(r.id),
            Self::RecipeRequirementsFloat(r) => u64::from(r.id),
            Self::RecipeRequirementsIID(r) => u64::from(r.id),
            Self::RecipeRequirementsInt(r) => u64::from(r.id),
            Self::RecipeRequirementsString(r) => u64::from(r.id),
            Self::Spell(r) => u64::from(r.id),
            Self::TreasureDeath(r) => u64::from(r.id),
            Self::TreasureGemCount(r) => u64::from(r.id),
            Self::TreasureMaterialBase(r) => u64::from(r.id),
            Self::TreasureMaterialColor(r) => u64::from(r.id),
            Self::TreasureMaterialGroups(r) => u64::from(r.id),
            Self::TreasureWielded(r) => u64::from(r.id),
            Self::Version(r) => u64::from(r.id),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::CookBook(r) => {
                if r.last_modified.len() <= 1048576 {
                    Ok(())
                } else {
                    Err("nonfinite or oversized cook_book field".into())
                }
            }
            Self::Encounter(r) => {
                if r.last_modified.len() <= 1048576 {
                    Ok(())
                } else {
                    Err("nonfinite or oversized encounter field".into())
                }
            }
            Self::Event(r) => {
                if r.name.len() <= 1048576 && r.last_modified.len() <= 1048576 {
                    Ok(())
                } else {
                    Err("nonfinite or oversized event field".into())
                }
            }
            Self::HousePortal(r) => {
                if r.origin_x.is_finite()
                    && r.origin_y.is_finite()
                    && r.origin_z.is_finite()
                    && r.angles_w.is_finite()
                    && r.angles_x.is_finite()
                    && r.angles_y.is_finite()
                    && r.angles_z.is_finite()
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized house_portal field".into())
                }
            }
            Self::LandblockInstance(r) => {
                if r.origin_x.is_finite()
                    && r.origin_y.is_finite()
                    && r.origin_z.is_finite()
                    && r.angles_w.is_finite()
                    && r.angles_x.is_finite()
                    && r.angles_y.is_finite()
                    && r.angles_z.is_finite()
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized landblock_instance field".into())
                }
            }
            Self::LandblockInstanceLink(r) => {
                if r.last_modified.len() <= 1048576 {
                    Ok(())
                } else {
                    Err("nonfinite or oversized landblock_instance_link field".into())
                }
            }
            Self::PointsOfInterest(r) => {
                if r.name.len() <= 1048576 && r.last_modified.len() <= 1048576 {
                    Ok(())
                } else {
                    Err("nonfinite or oversized points_of_interest field".into())
                }
            }
            Self::Quest(r) => {
                if r.name.len() <= 1048576
                    && r.message.as_ref().is_none_or(|v| v.len() <= 1048576)
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized quest field".into())
                }
            }
            Self::Recipe(r) => {
                if r.success_message
                    .as_ref()
                    .is_none_or(|v| v.len() <= 1048576)
                    && r.fail_message.as_ref().is_none_or(|v| v.len() <= 1048576)
                    && r.success_destroy_source_chance.is_finite()
                    && r.success_destroy_source_message
                        .as_ref()
                        .is_none_or(|v| v.len() <= 1048576)
                    && r.success_destroy_target_chance.is_finite()
                    && r.success_destroy_target_message
                        .as_ref()
                        .is_none_or(|v| v.len() <= 1048576)
                    && r.fail_destroy_source_chance.is_finite()
                    && r.fail_destroy_source_message
                        .as_ref()
                        .is_none_or(|v| v.len() <= 1048576)
                    && r.fail_destroy_target_chance.is_finite()
                    && r.fail_destroy_target_message
                        .as_ref()
                        .is_none_or(|v| v.len() <= 1048576)
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe field".into())
                }
            }
            Self::RecipeMod(_) => Ok(()),
            Self::RecipeModsBool(_) => Ok(()),
            Self::RecipeModsDID(_) => Ok(()),
            Self::RecipeModsFloat(r) => {
                if r.value.is_finite() {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_mods_float field".into())
                }
            }
            Self::RecipeModsIID(_) => Ok(()),
            Self::RecipeModsInt(_) => Ok(()),
            Self::RecipeModsString(r) => {
                if r.value.as_ref().is_none_or(|v| v.len() <= 1048576) {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_mods_string field".into())
                }
            }
            Self::RecipeRequirementsBool(r) => {
                if r.message.as_ref().is_none_or(|v| v.len() <= 1048576) {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_requirements_bool field".into())
                }
            }
            Self::RecipeRequirementsDID(r) => {
                if r.message.as_ref().is_none_or(|v| v.len() <= 1048576) {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_requirements_d_i_d field".into())
                }
            }
            Self::RecipeRequirementsFloat(r) => {
                if r.value.is_finite() && r.message.as_ref().is_none_or(|v| v.len() <= 1048576) {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_requirements_float field".into())
                }
            }
            Self::RecipeRequirementsIID(r) => {
                if r.message.as_ref().is_none_or(|v| v.len() <= 1048576) {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_requirements_i_i_d field".into())
                }
            }
            Self::RecipeRequirementsInt(r) => {
                if r.message.as_ref().is_none_or(|v| v.len() <= 1048576) {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_requirements_int field".into())
                }
            }
            Self::RecipeRequirementsString(r) => {
                if r.value.as_ref().is_none_or(|v| v.len() <= 1048576)
                    && r.message.as_ref().is_none_or(|v| v.len() <= 1048576)
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized recipe_requirements_string field".into())
                }
            }
            Self::Spell(r) => {
                if r.name.len() <= 1048576
                    && r.stat_mod_val.is_none_or(|v| v.is_finite())
                    && r.spread_angle.is_none_or(|v| v.is_finite())
                    && r.vertical_angle.is_none_or(|v| v.is_finite())
                    && r.default_launch_angle.is_none_or(|v| v.is_finite())
                    && r.create_offset_origin_x.is_none_or(|v| v.is_finite())
                    && r.create_offset_origin_y.is_none_or(|v| v.is_finite())
                    && r.create_offset_origin_z.is_none_or(|v| v.is_finite())
                    && r.padding_origin_x.is_none_or(|v| v.is_finite())
                    && r.padding_origin_y.is_none_or(|v| v.is_finite())
                    && r.padding_origin_z.is_none_or(|v| v.is_finite())
                    && r.dims_origin_x.is_none_or(|v| v.is_finite())
                    && r.dims_origin_y.is_none_or(|v| v.is_finite())
                    && r.dims_origin_z.is_none_or(|v| v.is_finite())
                    && r.peturbation_origin_x.is_none_or(|v| v.is_finite())
                    && r.peturbation_origin_y.is_none_or(|v| v.is_finite())
                    && r.peturbation_origin_z.is_none_or(|v| v.is_finite())
                    && r.slayer_damage_bonus.is_none_or(|v| v.is_finite())
                    && r.crit_freq.is_none_or(|v| v.is_finite())
                    && r.crit_multiplier.is_none_or(|v| v.is_finite())
                    && r.elemental_modifier.is_none_or(|v| v.is_finite())
                    && r.drain_percentage.is_none_or(|v| v.is_finite())
                    && r.damage_ratio.is_none_or(|v| v.is_finite())
                    && r.proportion.is_none_or(|v| v.is_finite())
                    && r.loss_percent.is_none_or(|v| v.is_finite())
                    && r.position_origin_x.is_none_or(|v| v.is_finite())
                    && r.position_origin_y.is_none_or(|v| v.is_finite())
                    && r.position_origin_z.is_none_or(|v| v.is_finite())
                    && r.position_angles_w.is_none_or(|v| v.is_finite())
                    && r.position_angles_x.is_none_or(|v| v.is_finite())
                    && r.position_angles_y.is_none_or(|v| v.is_finite())
                    && r.position_angles_z.is_none_or(|v| v.is_finite())
                    && r.power_variance.is_none_or(|v| v.is_finite())
                    && r.number_variance.is_none_or(|v| v.is_finite())
                    && r.dot_duration.is_none_or(|v| v.is_finite())
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized spell field".into())
                }
            }
            Self::TreasureDeath(r) => {
                if r.loot_quality_mod.is_finite() && r.last_modified.len() <= 1048576 {
                    Ok(())
                } else {
                    Err("nonfinite or oversized treasure_death field".into())
                }
            }
            Self::TreasureGemCount(r) => {
                if r.chance.is_finite() {
                    Ok(())
                } else {
                    Err("nonfinite or oversized treasure_gem_count field".into())
                }
            }
            Self::TreasureMaterialBase(r) => {
                if r.probability.is_finite() {
                    Ok(())
                } else {
                    Err("nonfinite or oversized treasure_material_base field".into())
                }
            }
            Self::TreasureMaterialColor(r) => {
                if r.probability.is_finite() {
                    Ok(())
                } else {
                    Err("nonfinite or oversized treasure_material_color field".into())
                }
            }
            Self::TreasureMaterialGroups(r) => {
                if r.probability.is_finite() {
                    Ok(())
                } else {
                    Err("nonfinite or oversized treasure_material_groups field".into())
                }
            }
            Self::TreasureWielded(r) => {
                if r.shade.is_finite()
                    && r.stack_size_variance.is_finite()
                    && r.probability.is_finite()
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized treasure_wielded field".into())
                }
            }
            Self::Version(r) => {
                if r.base_version.as_ref().is_none_or(|v| v.len() <= 1048576)
                    && r.patch_version.as_ref().is_none_or(|v| v.len() <= 1048576)
                    && r.last_modified.len() <= 1048576
                {
                    Ok(())
                } else {
                    Err("nonfinite or oversized version field".into())
                }
            }
        }
    }
}
