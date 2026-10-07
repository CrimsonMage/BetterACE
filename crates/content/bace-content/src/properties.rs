//! Typed fields from official ACE.Entity.Models at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! AGPL-3.0-only; source: https://github.com/ACEmulator/ACE/tree/47edade3bd3f6044b676d4eb877c4965c7eda62b/Source/ACE.Entity/Models
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnimationPart {
    pub index: u8,
    pub animation_id: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Attribute {
    pub init_level: u32,
    pub level_from_cp: u32,
    pub cp_spent: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecondaryAttribute {
    pub init_level: u32,
    pub level_from_cp: u32,
    pub cp_spent: u32,
    pub current_level: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BodyPart {
    pub d_type: i32,
    pub d_val: i32,
    pub d_var: f32,
    pub base_armor: i32,
    pub armor_vs_slash: i32,
    pub armor_vs_pierce: i32,
    pub armor_vs_bludgeon: i32,
    pub armor_vs_cold: i32,
    pub armor_vs_fire: i32,
    pub armor_vs_acid: i32,
    pub armor_vs_electric: i32,
    pub armor_vs_nether: i32,
    pub bh: i32,
    pub hlf: f32,
    pub mlf: f32,
    pub llf: f32,
    pub hrf: f32,
    pub mrf: f32,
    pub lrf: f32,
    pub hlb: f32,
    pub mlb: f32,
    pub llb: f32,
    pub hrb: f32,
    pub mrb: f32,
    pub lrb: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Book {
    pub max_num_pages: i32,
    pub max_num_chars_per_page: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BookPage {
    pub legacy_page_id: Option<u32>,
    pub author_id: u32,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub author_account: Option<String>,
    pub ignore_author: bool,
    #[serde(default)]
    pub page_text: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CreateListEntry {
    pub database_record_id: u32,
    pub destination_type: i32,
    pub weenie_class_id: u32,
    pub stack_size: i32,
    pub palette: i8,
    pub shade: f32,
    pub try_to_bond: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Emote {
    pub legacy_category_key: Option<i32>,
    pub database_record_id: u32,
    pub category: i32,
    pub probability: f32,
    #[serde(default)]
    pub weenie_class_id: Option<u32>,
    #[serde(default)]
    pub style: Option<u32>,
    #[serde(default)]
    pub substyle: Option<u32>,
    #[serde(default)]
    pub quest: Option<String>,
    #[serde(default)]
    pub vendor_type: Option<i32>,
    #[serde(default)]
    pub min_health: Option<f32>,
    #[serde(default)]
    pub max_health: Option<f32>,
    #[serde(default)]
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub actions: Vec<EmoteAction>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EmoteAction {
    pub legacy_order: Option<u32>,
    pub database_record_id: u32,
    pub r#type: u32,
    pub delay: f32,
    pub extent: f32,
    #[serde(default)]
    pub motion: Option<u32>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub test_string: Option<String>,
    #[serde(default)]
    pub min: Option<i32>,
    #[serde(default)]
    pub max: Option<i32>,
    #[serde(default)]
    pub min64: Option<i64>,
    #[serde(default)]
    pub max64: Option<i64>,
    #[serde(default)]
    pub min_dbl: Option<f64>,
    #[serde(default)]
    pub max_dbl: Option<f64>,
    #[serde(default)]
    pub stat: Option<i32>,
    #[serde(default)]
    pub display: Option<bool>,
    #[serde(default)]
    pub amount: Option<i32>,
    #[serde(default)]
    pub amount64: Option<i64>,
    #[serde(default)]
    pub hero_xp64: Option<i64>,
    #[serde(default)]
    pub percent: Option<f64>,
    #[serde(default)]
    pub spell_id: Option<i32>,
    #[serde(default)]
    pub wealth_rating: Option<i32>,
    #[serde(default)]
    pub treasure_class: Option<i32>,
    #[serde(default)]
    pub treasure_type: Option<i32>,
    #[serde(default)]
    pub p_script: Option<u32>,
    #[serde(default)]
    pub sound: Option<u32>,
    #[serde(default)]
    pub destination_type: Option<i8>,
    #[serde(default)]
    pub weenie_class_id: Option<u32>,
    #[serde(default)]
    pub stack_size: Option<i32>,
    #[serde(default)]
    pub palette: Option<i32>,
    #[serde(default)]
    pub shade: Option<f32>,
    #[serde(default)]
    pub try_to_bond: Option<bool>,
    #[serde(default)]
    pub obj_cell_id: Option<u32>,
    #[serde(default)]
    pub origin_x: Option<f32>,
    #[serde(default)]
    pub origin_y: Option<f32>,
    #[serde(default)]
    pub origin_z: Option<f32>,
    #[serde(default)]
    pub angles_w: Option<f32>,
    #[serde(default)]
    pub angles_x: Option<f32>,
    #[serde(default)]
    pub angles_y: Option<f32>,
    #[serde(default)]
    pub angles_z: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Generator {
    pub legacy_slot: Option<u32>,
    pub database_record_id: u32,
    pub probability: f32,
    pub weenie_class_id: u32,
    #[serde(default)]
    pub delay: Option<f32>,
    pub init_create: i32,
    pub max_create: i32,
    pub when_create: u32,
    pub where_create: u32,
    #[serde(default)]
    pub stack_size: Option<i32>,
    #[serde(default)]
    pub palette_id: Option<u32>,
    #[serde(default)]
    pub shade: Option<f32>,
    #[serde(default)]
    pub obj_cell_id: Option<u32>,
    #[serde(default)]
    pub origin_x: Option<f32>,
    #[serde(default)]
    pub origin_y: Option<f32>,
    #[serde(default)]
    pub origin_z: Option<f32>,
    #[serde(default)]
    pub angles_w: Option<f32>,
    #[serde(default)]
    pub angles_x: Option<f32>,
    #[serde(default)]
    pub angles_y: Option<f32>,
    #[serde(default)]
    pub angles_z: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Palette {
    pub sub_palette_id: u32,
    pub offset: u16,
    pub length: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Position {
    pub obj_cell_id: u32,
    pub position_x: f32,
    pub position_y: f32,
    pub position_z: f32,
    pub rotation_w: f32,
    pub rotation_x: f32,
    pub rotation_y: f32,
    pub rotation_z: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Skill {
    pub level_from_pp: u16,
    pub sac: u32,
    pub pp: u32,
    pub init_level: u32,
    pub resistance_at_last_check: u32,
    pub last_used_time: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextureMap {
    pub part_index: u8,
    pub old_texture: u32,
    pub new_texture: u32,
}

/// Authoring-only legacy metadata retained as typed fields, never a JSON blob.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthoringMetadata {
    pub modified_by: Option<String>,
    pub user_change_summary: Option<String>,
    pub is_done: Option<bool>,
    pub comments: Option<String>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub changelog: Vec<AuthoringChange>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthoringChange {
    pub created: Option<String>,
    pub author: Option<String>,
    pub comment: Option<String>,
}
