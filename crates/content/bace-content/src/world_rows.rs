//! Frozen world rows from official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! AGPL-3.0-only. Generated from data/world-base.sql; field order is schema 1.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookBookRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub source_w_c_i_d: u32,
    pub target_w_c_i_d: u32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterRowV1 {
    pub id: u32,
    pub landblock: i32,
    pub weenie_class_id: u32,
    pub cell_x: i32,
    pub cell_y: i32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRowV1 {
    pub id: u32,
    pub name: String,
    pub start_time: i32,
    pub end_time: i32,
    pub state: i32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HousePortalRowV1 {
    pub id: u32,
    pub house_id: u32,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandblockInstanceRowV1 {
    pub guid: u32,
    pub landblock: i32,
    pub weenie_class_id: u32,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
    pub is_link_child: bool,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandblockInstanceLinkRowV1 {
    pub id: u32,
    pub parent_guid: u32,
    pub child_guid: u32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointsOfInterestRowV1 {
    pub id: u32,
    pub name: String,
    pub weenie_class_id: u32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestRowV1 {
    pub id: u32,
    pub name: String,
    pub min_delta: u32,
    pub max_solves: i32,
    pub message: Option<String>,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRowV1 {
    pub id: u32,
    pub unknown_1: u32,
    pub skill: u32,
    pub difficulty: u32,
    pub salvage_type: u32,
    pub success_w_c_i_d: u32,
    pub success_amount: u32,
    pub success_message: Option<String>,
    pub fail_w_c_i_d: u32,
    pub fail_amount: u32,
    pub fail_message: Option<String>,
    pub success_destroy_source_chance: f64,
    pub success_destroy_source_amount: u32,
    pub success_destroy_source_message: Option<String>,
    pub success_destroy_target_chance: f64,
    pub success_destroy_target_amount: u32,
    pub success_destroy_target_message: Option<String>,
    pub fail_destroy_source_chance: f64,
    pub fail_destroy_source_amount: u32,
    pub fail_destroy_source_message: Option<String>,
    pub fail_destroy_target_chance: f64,
    pub fail_destroy_target_amount: u32,
    pub fail_destroy_target_message: Option<String>,
    pub data_id: u32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub executes_on_success: bool,
    pub health: i32,
    pub stamina: i32,
    pub mana: i32,
    pub unknown_7: bool,
    pub data_id: i32,
    pub unknown_9: i32,
    pub instance_id: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModsBoolRowV1 {
    pub id: u32,
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: bool,
    pub r#enum: i32,
    pub source: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModsDIDRowV1 {
    pub id: u32,
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub source: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModsFloatRowV1 {
    pub id: u32,
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: f64,
    pub r#enum: i32,
    pub source: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModsIIDRowV1 {
    pub id: u32,
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub source: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModsIntRowV1 {
    pub id: u32,
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: i32,
    pub r#enum: i32,
    pub source: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeModsStringRowV1 {
    pub id: u32,
    pub recipe_mod_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: Option<String>,
    pub r#enum: i32,
    pub source: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequirementsBoolRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: bool,
    pub r#enum: i32,
    pub message: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequirementsDIDRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub message: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequirementsFloatRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: f64,
    pub r#enum: i32,
    pub message: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequirementsIIDRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: u32,
    pub r#enum: i32,
    pub message: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequirementsIntRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: i32,
    pub r#enum: i32,
    pub message: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequirementsStringRowV1 {
    pub id: u32,
    pub recipe_id: u32,
    pub index: i8,
    pub stat: i32,
    pub value: Option<String>,
    pub r#enum: i32,
    pub message: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellRowV1 {
    pub id: u32,
    pub name: String,
    pub stat_mod_type: Option<u32>,
    pub stat_mod_key: Option<u32>,
    pub stat_mod_val: Option<f32>,
    pub e_type: Option<u32>,
    pub base_intensity: Option<i32>,
    pub variance: Option<i32>,
    pub wcid: Option<u32>,
    pub num_projectiles: Option<i32>,
    pub num_projectiles_variance: Option<i32>,
    pub spread_angle: Option<f32>,
    pub vertical_angle: Option<f32>,
    pub default_launch_angle: Option<f32>,
    pub non_tracking: Option<bool>,
    pub create_offset_origin_x: Option<f32>,
    pub create_offset_origin_y: Option<f32>,
    pub create_offset_origin_z: Option<f32>,
    pub padding_origin_x: Option<f32>,
    pub padding_origin_y: Option<f32>,
    pub padding_origin_z: Option<f32>,
    pub dims_origin_x: Option<f32>,
    pub dims_origin_y: Option<f32>,
    pub dims_origin_z: Option<f32>,
    pub peturbation_origin_x: Option<f32>,
    pub peturbation_origin_y: Option<f32>,
    pub peturbation_origin_z: Option<f32>,
    pub imbued_effect: Option<u32>,
    pub slayer_creature_type: Option<i32>,
    pub slayer_damage_bonus: Option<f32>,
    pub crit_freq: Option<f64>,
    pub crit_multiplier: Option<f64>,
    pub ignore_magic_resist: Option<i32>,
    pub elemental_modifier: Option<f64>,
    pub drain_percentage: Option<f32>,
    pub damage_ratio: Option<f32>,
    pub damage_type: Option<i32>,
    pub boost: Option<i32>,
    pub boost_variance: Option<i32>,
    pub source: Option<i32>,
    pub destination: Option<i32>,
    pub proportion: Option<f32>,
    pub loss_percent: Option<f32>,
    pub source_loss: Option<i32>,
    pub transfer_cap: Option<i32>,
    pub max_boost_allowed: Option<i32>,
    pub transfer_bitfield: Option<u32>,
    pub index: Option<i32>,
    pub link: Option<i32>,
    pub position_obj_cell_id: Option<u32>,
    pub position_origin_x: Option<f32>,
    pub position_origin_y: Option<f32>,
    pub position_origin_z: Option<f32>,
    pub position_angles_w: Option<f32>,
    pub position_angles_x: Option<f32>,
    pub position_angles_y: Option<f32>,
    pub position_angles_z: Option<f32>,
    pub min_power: Option<i32>,
    pub max_power: Option<i32>,
    pub power_variance: Option<f32>,
    pub dispel_school: Option<i32>,
    pub align: Option<i32>,
    pub number: Option<i32>,
    pub number_variance: Option<f32>,
    pub dot_duration: Option<f64>,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureDeathRowV1 {
    pub id: u32,
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
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureGemCountRowV1 {
    pub id: u32,
    pub gem_code: u8,
    pub tier: i32,
    pub count: i32,
    pub chance: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureMaterialBaseRowV1 {
    pub id: u32,
    pub material_code: u32,
    pub tier: u32,
    pub probability: f32,
    pub material_id: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureMaterialColorRowV1 {
    pub id: u32,
    pub material_id: u32,
    pub color_code: u32,
    pub palette_template: u32,
    pub probability: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureMaterialGroupsRowV1 {
    pub id: u32,
    pub material_group: u32,
    pub tier: u32,
    pub probability: f32,
    pub material_id: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasureWieldedRowV1 {
    pub id: u32,
    pub treasure_type: u32,
    pub weenie_class_id: u32,
    pub palette_id: u32,
    pub unknown_1: u32,
    pub shade: f32,
    pub stack_size: i32,
    pub stack_size_variance: f32,
    pub probability: f32,
    pub unknown_3: u32,
    pub unknown_4: u32,
    pub unknown_5: u32,
    pub set_start: bool,
    pub has_sub_set: bool,
    pub continues_previous_set: bool,
    pub unknown_9: u32,
    pub unknown_10: u32,
    pub unknown_11: u32,
    pub unknown_12: u32,
    pub last_modified: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRowV1 {
    pub id: u32,
    pub base_version: Option<String>,
    pub patch_version: Option<String>,
    pub last_modified: String,
}
