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
