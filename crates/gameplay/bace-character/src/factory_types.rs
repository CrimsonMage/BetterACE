//! Neutral prepared asset projections. Runtime maps verified DAT/content into
//! these immutable inputs; the character domain never opens DATs or a database.
use crate::CreationRules;
use bace_content::{Position, WeenieV1};
use bace_gameplay_api::CreationAllocation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceTextures {
    pub old: u32,
    pub new: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedHair {
    pub bald: bool,
    pub alternate_setup: u32,
    pub head_object: Option<u32>,
    pub texture: Option<FaceTextures>,
    pub multiple_parts: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedEyes {
    pub normal: FaceTextures,
    pub bald: FaceTextures,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedGender {
    pub gender: u32,
    pub scale: u32,
    pub setup: u32,
    pub motion: u32,
    pub sound: u32,
    pub physics: u32,
    pub combat: u32,
    pub palette_base: u32,
    pub hair: Vec<PreparedHair>,
    pub hair_palette_sets: Vec<Vec<u32>>,
    pub skin_palettes: Vec<u32>,
    pub eye_palettes: Vec<u32>,
    pub eyes: Vec<PreparedEyes>,
    pub noses: Vec<FaceTextures>,
    pub mouths: Vec<FaceTextures>,
    /// Headgear, shirt, pants, footwear. Content IDs follow DAT selection order.
    pub clothing: [Vec<u32>; 4],
    pub clothing_colors: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceSelection {
    pub hair_style: u32,
    pub hair_color: u32,
    pub hair_hue: f64,
    pub skin_hue: f64,
    pub eyes: u32,
    pub eye_color: u32,
    pub nose: u32,
    pub mouth: u32,
    pub clothing: [ClothingSelection; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClothingSelection {
    pub style: u32,
    pub color: u32,
    pub hue: f64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterTemplate {
    pub name: String,
    pub title: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterStart {
    pub area: u32,
    pub location: Position,
    pub instantiation: Position,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StarterItem {
    pub template: WeenieV1,
    pub revision: u64,
    pub clothing_icons: Vec<(u32, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillGear {
    pub skill: u32,
    pub heritage: Option<u32>,
    pub template: u32,
    pub count: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillSpell {
    pub skill: u32,
    pub spell: u32,
    pub specialized_only: bool,
}

pub struct PreparedCreationAssets {
    pub heritage: u32,
    pub heritage_name: String,
    pub rules: CreationRules,
    pub genders: Vec<PreparedGender>,
    pub templates: Vec<CharacterTemplate>,
    pub starts: Vec<CharacterStart>,
    pub human: WeenieV1,
    pub human_revision: u64,
    pub items: Vec<StarterItem>,
    pub skill_gear: Vec<SkillGear>,
    pub skill_spells: Vec<SkillSpell>,
    /// DAT secondary-attribute formula projections: X, divisor Z, Attr1, Attr2.
    pub vital_formulas: [VitalFormula; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalFormula {
    pub enabled: bool,
    pub divisor: u32,
    pub attribute1: u32,
    pub attribute2: u32,
}
pub struct CharacterCreateRequest {
    pub account: u64,
    pub entity: u32,
    pub name: String,
    pub heritage: u32,
    pub gender: u32,
    pub template_index: u32,
    pub start_area: u32,
    pub allocation: CreationAllocation,
    pub appearance: AppearanceSelection,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterMetadata {
    pub hair_texture: u32,
    pub default_hair_texture: u32,
    pub titles: Vec<u32>,
    pub current_title: u32,
    pub options1: u32,
    pub options2: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPossession {
    pub entity: u32,
    pub template_revision: u64,
    pub equipped: u32,
    pub state: WeenieV1,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedCharacter {
    pub account: u64,
    pub entity: u32,
    pub name: String,
    pub template_revision: u64,
    pub state: WeenieV1,
    pub metadata: CharacterMetadata,
    pub possessions: Vec<PreparedPossession>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactoryError {
    Identity,
    NameNotApproved,
    UnsupportedHeritage,
    MissingAsset,
    InvalidSelection,
    InvalidHue,
    InvalidContent,
    InvalidPosition,
    Capacity,
    Allocation(crate::CreationRejection),
    InsufficientIds,
    DuplicateId,
    Overflow,
}
