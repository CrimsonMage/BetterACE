use serde::{Deserialize, Serialize};

use crate::properties::*;

/// Frozen native V1 representation. Changing field order/type requires a new DTO
/// and an explicit storage schema migration; never persist gameplay structs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeenieV1 {
    pub schema_version: u16,
    pub weenie_id: u32,
    pub class_name: String,
    pub weenie_type: u32,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub properties: SparseProperties,
}

pub type WeenieTemplate = WeenieV1;

/// Numeric property IDs survive unknown future enum variants without coercion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property<T, K = u32> {
    pub id: K,
    pub value: T,
}

/// Dictionary families are canonicalized by numeric ID. Sequence families remain
/// in source order, including emote actions and repeated create-list entries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SparseProperties {
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub bools: Vec<Property<bool>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub data_ids: Vec<Property<u32>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub floats: Vec<Property<f64>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub instance_ids: Vec<Property<u32>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub ints: Vec<Property<i32>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub int64s: Vec<Property<i64>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub strings: Vec<Property<String>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub positions: Vec<Property<Position>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub spell_book: Vec<Property<f32, i32>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub animation_parts: Vec<AnimationPart>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub palettes: Vec<Palette>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub texture_maps: Vec<TextureMap>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub create_list: Vec<CreateListEntry>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub emotes: Vec<Emote>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub event_filter: Vec<i32>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub generators: Vec<Generator>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub attributes: Vec<Property<Attribute>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub secondary_attributes: Vec<Property<SecondaryAttribute>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub body_parts: Vec<Property<BodyPart, i32>>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub skills: Vec<Property<Skill, i32>>,
    pub book: Option<Book>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub book_pages: Vec<BookPage>,
    pub authoring_metadata: Option<AuthoringMetadata>,
}
