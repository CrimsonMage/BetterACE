//! Native TOML authoring and deterministic binary compilation.
mod animation_swap;
mod character_start;
mod clothing_patch;
pub use animation_swap::{compile_animation_swap, decode_animation_swap, parse_animation_swap};
mod compiler;
mod creature_name_validation;
pub use character_start::{compile_character_start, decode_character_start, parse_character_start};
pub use character_start::{compile_creature_names, decode_creature_names};
pub use character_start::{compile_template_classes, decode_template_classes};
mod packs;

pub use clothing_patch::{compile_clothing_patch, decode_clothing_patch, parse_clothing_patch};
pub use compiler::{ToolError, compile, compile_template, decode, export, export_binary, parse};
pub use packs::{PackBuild, build_weenie_pack};
mod world_pack;
pub use world_pack::{
    build_world_pack, compile_landblock_index, compile_world_record, decode_landblock_index,
    decode_world_record,
};

pub use world_pack::{export_world_record, parse_world_record};

pub use world_pack::{compile_instance_link_index, decode_instance_link_index};
mod world_index_validation;
pub use world_index_validation::validate_world_pack_indexes;
mod loot_tables;
mod treasure_table_set;
pub use loot_tables::{
    build_loot_pack, compile_loot_graph, compile_rare_profile, decode_loot_graph,
    decode_rare_profile, parse_loot_graph, parse_rare_profile,
};
pub use treasure_table_set::{
    compile_treasure_table_set, decode_treasure_table_set, parse_treasure_table_set,
};
