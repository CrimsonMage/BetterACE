//! DAT decoding, validation and immutable asset access.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod archive;
mod landblock;

pub use archive::{DatArchive, DatError, DatHeader, DatRecord, fingerprint};
pub use landblock::Landblock;
mod skill_table;
mod table_reader;
mod xp_table;
pub use skill_table::{SkillBase, SkillFormula, SkillTable};
pub use table_reader::{DatTableLimits, DatTableVersion};
pub use xp_table::XpTable;

mod appearance;
mod chargen;
pub use appearance::{
    AnimationPart, CreationGear, CreationGender, EyeStrip, FaceStrip, HairStyle, ObjectDescription,
    SubPalette, TextureChange,
};
pub use chargen::{
    CharGen, CreationPosition, CreationSkill, CreationTemplate, HeritageGroup, StarterArea,
};

mod clothing_assets;
mod model_assets;
pub use clothing_assets::{
    ClothingPalette, ClothingPart, ClothingTable, ClothingTemplate, DatPalette, DatPaletteSet,
};
pub use model_assets::{GraphicsObject, ModelFrame, ModelPolygon, ModelSetup, ModelVertex};
mod texture_assets;
pub use texture_assets::{DatSurface, DatTexture, decode_texture_list};
mod vital_table;
pub use vital_table::VitalTable;
mod bsp;
mod env_cell;
pub use bsp::{BspNode, BspSphere, BspTree, BspTreeKind};
pub use env_cell::{CellPortal, EnvCell, StaticObject};
mod animation;
mod animation_hooks;
mod motion_table;
pub use animation::{Animation, AnimationFrame};
pub use animation_hooks::{AnimationHook, AnimationHookPayload, AttackCone};
pub use motion_table::{AnimationSegment, MotionData, MotionTable};
mod environment;
pub use environment::{CellGeometry, Environment};
mod collision_setup;
mod spell_components;
mod spell_formula;
mod spell_strings;
mod spell_table;
pub use collision_setup::{CollisionCylinder, CollisionSetup, SetupLight, SetupLocation};
pub use spell_components::{SpellComponent, SpellComponents};
pub use spell_strings::spell_hash_cp1252;
pub use spell_table::{SpellBase, SpellTable};

mod taboo_table;
pub use taboo_table::{TabooEntry, TabooTable};

mod combat_motion_table;
pub use combat_motion_table::{CombatManeuver, CombatManeuverTable};

mod region_land;
pub use region_land::RegionLand;

mod dual_did_mapper;
pub use dual_did_mapper::DualDidMapper;

mod landblock_info;
pub use landblock_info::{BuildingInfo, BuildingPortal, LandblockInfo};

mod contract_table;
pub use contract_table::{Contract, ContractLocation, ContractTable};

mod quality_filter;
pub use quality_filter::QualityFilter;
