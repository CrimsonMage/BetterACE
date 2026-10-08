//! Frozen world rows from official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! AGPL-3.0-only. Generated from data/world-base.sql; field order is schema 1.
use crate::sql_specs::TableSpec;
use crate::{SqlStagingError, mariadb::IsolatedMariaDb};
use bace_content::WorldRecordV1;
pub(crate) const WORLD_TABLES: &[TableSpec] = &[
    TableSpec {
        name: "cook_book",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("source_W_C_I_D", "source_w_c_i_d"),
            ("target_W_C_I_D", "target_w_c_i_d"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "encounter",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("landblock", "landblock"),
            ("weenie_Class_Id", "weenie_class_id"),
            ("cell_X", "cell_x"),
            ("cell_Y", "cell_y"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "event",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("name", "name"),
            ("start_Time", "start_time"),
            ("end_Time", "end_time"),
            ("state", "state"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "house_portal",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("house_Id", "house_id"),
            ("obj_Cell_Id", "obj_cell_id"),
            ("origin_X", "origin_x"),
            ("origin_Y", "origin_y"),
            ("origin_Z", "origin_z"),
            ("angles_W", "angles_w"),
            ("angles_X", "angles_x"),
            ("angles_Y", "angles_y"),
            ("angles_Z", "angles_z"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "landblock_instance",
        native: "",
        key: "",
        order: "guid",
        fields: &[
            ("guid", "guid"),
            ("landblock", "landblock"),
            ("weenie_Class_Id", "weenie_class_id"),
            ("obj_Cell_Id", "obj_cell_id"),
            ("origin_X", "origin_x"),
            ("origin_Y", "origin_y"),
            ("origin_Z", "origin_z"),
            ("angles_W", "angles_w"),
            ("angles_X", "angles_x"),
            ("angles_Y", "angles_y"),
            ("angles_Z", "angles_z"),
            ("is_Link_Child", "is_link_child"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "landblock_instance_link",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("parent_GUID", "parent_guid"),
            ("child_GUID", "child_guid"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "points_of_interest",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("name", "name"),
            ("weenie_Class_Id", "weenie_class_id"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "quest",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("name", "name"),
            ("min_Delta", "min_delta"),
            ("max_Solves", "max_solves"),
            ("message", "message"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "recipe",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("unknown_1", "unknown_1"),
            ("skill", "skill"),
            ("difficulty", "difficulty"),
            ("salvage_Type", "salvage_type"),
            ("success_W_C_I_D", "success_w_c_i_d"),
            ("success_Amount", "success_amount"),
            ("success_Message", "success_message"),
            ("fail_W_C_I_D", "fail_w_c_i_d"),
            ("fail_Amount", "fail_amount"),
            ("fail_Message", "fail_message"),
            (
                "success_Destroy_Source_Chance",
                "success_destroy_source_chance",
            ),
            (
                "success_Destroy_Source_Amount",
                "success_destroy_source_amount",
            ),
            (
                "success_Destroy_Source_Message",
                "success_destroy_source_message",
            ),
            (
                "success_Destroy_Target_Chance",
                "success_destroy_target_chance",
            ),
            (
                "success_Destroy_Target_Amount",
                "success_destroy_target_amount",
            ),
            (
                "success_Destroy_Target_Message",
                "success_destroy_target_message",
            ),
            ("fail_Destroy_Source_Chance", "fail_destroy_source_chance"),
            ("fail_Destroy_Source_Amount", "fail_destroy_source_amount"),
            ("fail_Destroy_Source_Message", "fail_destroy_source_message"),
            ("fail_Destroy_Target_Chance", "fail_destroy_target_chance"),
            ("fail_Destroy_Target_Amount", "fail_destroy_target_amount"),
            ("fail_Destroy_Target_Message", "fail_destroy_target_message"),
            ("data_Id", "data_id"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "recipe_mod",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("executes_On_Success", "executes_on_success"),
            ("health", "health"),
            ("stamina", "stamina"),
            ("mana", "mana"),
            ("unknown_7", "unknown_7"),
            ("data_Id", "data_id"),
            ("unknown_9", "unknown_9"),
            ("instance_Id", "instance_id"),
        ],
    },
    TableSpec {
        name: "recipe_mods_bool",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Mod_Id", "recipe_mod_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("source", "source"),
        ],
    },
    TableSpec {
        name: "recipe_mods_d_i_d",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Mod_Id", "recipe_mod_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("source", "source"),
        ],
    },
    TableSpec {
        name: "recipe_mods_float",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Mod_Id", "recipe_mod_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("source", "source"),
        ],
    },
    TableSpec {
        name: "recipe_mods_i_i_d",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Mod_Id", "recipe_mod_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("source", "source"),
        ],
    },
    TableSpec {
        name: "recipe_mods_int",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Mod_Id", "recipe_mod_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("source", "source"),
        ],
    },
    TableSpec {
        name: "recipe_mods_string",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Mod_Id", "recipe_mod_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("source", "source"),
        ],
    },
    TableSpec {
        name: "recipe_requirements_bool",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("message", "message"),
        ],
    },
    TableSpec {
        name: "recipe_requirements_d_i_d",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("message", "message"),
        ],
    },
    TableSpec {
        name: "recipe_requirements_float",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("message", "message"),
        ],
    },
    TableSpec {
        name: "recipe_requirements_i_i_d",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("message", "message"),
        ],
    },
    TableSpec {
        name: "recipe_requirements_int",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("message", "message"),
        ],
    },
    TableSpec {
        name: "recipe_requirements_string",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("recipe_Id", "recipe_id"),
            ("index", "index"),
            ("stat", "stat"),
            ("value", "value"),
            ("enum", "enum"),
            ("message", "message"),
        ],
    },
    TableSpec {
        name: "spell",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("name", "name"),
            ("stat_Mod_Type", "stat_mod_type"),
            ("stat_Mod_Key", "stat_mod_key"),
            ("stat_Mod_Val", "stat_mod_val"),
            ("e_Type", "e_type"),
            ("base_Intensity", "base_intensity"),
            ("variance", "variance"),
            ("wcid", "wcid"),
            ("num_Projectiles", "num_projectiles"),
            ("num_Projectiles_Variance", "num_projectiles_variance"),
            ("spread_Angle", "spread_angle"),
            ("vertical_Angle", "vertical_angle"),
            ("default_Launch_Angle", "default_launch_angle"),
            ("non_Tracking", "non_tracking"),
            ("create_Offset_Origin_X", "create_offset_origin_x"),
            ("create_Offset_Origin_Y", "create_offset_origin_y"),
            ("create_Offset_Origin_Z", "create_offset_origin_z"),
            ("padding_Origin_X", "padding_origin_x"),
            ("padding_Origin_Y", "padding_origin_y"),
            ("padding_Origin_Z", "padding_origin_z"),
            ("dims_Origin_X", "dims_origin_x"),
            ("dims_Origin_Y", "dims_origin_y"),
            ("dims_Origin_Z", "dims_origin_z"),
            ("peturbation_Origin_X", "peturbation_origin_x"),
            ("peturbation_Origin_Y", "peturbation_origin_y"),
            ("peturbation_Origin_Z", "peturbation_origin_z"),
            ("imbued_Effect", "imbued_effect"),
            ("slayer_Creature_Type", "slayer_creature_type"),
            ("slayer_Damage_Bonus", "slayer_damage_bonus"),
            ("crit_Freq", "crit_freq"),
            ("crit_Multiplier", "crit_multiplier"),
            ("ignore_Magic_Resist", "ignore_magic_resist"),
            ("elemental_Modifier", "elemental_modifier"),
            ("drain_Percentage", "drain_percentage"),
            ("damage_Ratio", "damage_ratio"),
            ("damage_Type", "damage_type"),
            ("boost", "boost"),
            ("boost_Variance", "boost_variance"),
            ("source", "source"),
            ("destination", "destination"),
            ("proportion", "proportion"),
            ("loss_Percent", "loss_percent"),
            ("source_Loss", "source_loss"),
            ("transfer_Cap", "transfer_cap"),
            ("max_Boost_Allowed", "max_boost_allowed"),
            ("transfer_Bitfield", "transfer_bitfield"),
            ("index", "index"),
            ("link", "link"),
            ("position_Obj_Cell_ID", "position_obj_cell_id"),
            ("position_Origin_X", "position_origin_x"),
            ("position_Origin_Y", "position_origin_y"),
            ("position_Origin_Z", "position_origin_z"),
            ("position_Angles_W", "position_angles_w"),
            ("position_Angles_X", "position_angles_x"),
            ("position_Angles_Y", "position_angles_y"),
            ("position_Angles_Z", "position_angles_z"),
            ("min_Power", "min_power"),
            ("max_Power", "max_power"),
            ("power_Variance", "power_variance"),
            ("dispel_School", "dispel_school"),
            ("align", "align"),
            ("number", "number"),
            ("number_Variance", "number_variance"),
            ("dot_Duration", "dot_duration"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "treasure_death",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("treasure_Type", "treasure_type"),
            ("tier", "tier"),
            ("loot_Quality_Mod", "loot_quality_mod"),
            ("unknown_Chances", "unknown_chances"),
            ("item_Chance", "item_chance"),
            ("item_Min_Amount", "item_min_amount"),
            ("item_Max_Amount", "item_max_amount"),
            (
                "item_Treasure_Type_Selection_Chances",
                "item_treasure_type_selection_chances",
            ),
            ("magic_Item_Chance", "magic_item_chance"),
            ("magic_Item_Min_Amount", "magic_item_min_amount"),
            ("magic_Item_Max_Amount", "magic_item_max_amount"),
            (
                "magic_Item_Treasure_Type_Selection_Chances",
                "magic_item_treasure_type_selection_chances",
            ),
            ("mundane_Item_Chance", "mundane_item_chance"),
            ("mundane_Item_Min_Amount", "mundane_item_min_amount"),
            ("mundane_Item_Max_Amount", "mundane_item_max_amount"),
            (
                "mundane_Item_Type_Selection_Chances",
                "mundane_item_type_selection_chances",
            ),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "treasure_gem_count",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("gem_Code", "gem_code"),
            ("tier", "tier"),
            ("count", "count"),
            ("chance", "chance"),
        ],
    },
    TableSpec {
        name: "treasure_material_base",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("material_Code", "material_code"),
            ("tier", "tier"),
            ("probability", "probability"),
            ("material_Id", "material_id"),
        ],
    },
    TableSpec {
        name: "treasure_material_color",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("material_Id", "material_id"),
            ("color_Code", "color_code"),
            ("palette_Template", "palette_template"),
            ("probability", "probability"),
        ],
    },
    TableSpec {
        name: "treasure_material_groups",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("material_Group", "material_group"),
            ("tier", "tier"),
            ("probability", "probability"),
            ("material_Id", "material_id"),
        ],
    },
    TableSpec {
        name: "treasure_wielded",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("treasure_Type", "treasure_type"),
            ("weenie_Class_Id", "weenie_class_id"),
            ("palette_Id", "palette_id"),
            ("unknown_1", "unknown_1"),
            ("shade", "shade"),
            ("stack_Size", "stack_size"),
            ("stack_Size_Variance", "stack_size_variance"),
            ("probability", "probability"),
            ("unknown_3", "unknown_3"),
            ("unknown_4", "unknown_4"),
            ("unknown_5", "unknown_5"),
            ("set_Start", "set_start"),
            ("has_Sub_Set", "has_sub_set"),
            ("continues_Previous_Set", "continues_previous_set"),
            ("unknown_9", "unknown_9"),
            ("unknown_10", "unknown_10"),
            ("unknown_11", "unknown_11"),
            ("unknown_12", "unknown_12"),
            ("last_Modified", "last_modified"),
        ],
    },
    TableSpec {
        name: "version",
        native: "",
        key: "",
        order: "id",
        fields: &[
            ("id", "id"),
            ("base_Version", "base_version"),
            ("patch_Version", "patch_version"),
            ("last_Modified", "last_modified"),
        ],
    },
];
pub(crate) fn extract(db: &IsolatedMariaDb) -> Result<Vec<WorldRecordV1>, SqlStagingError> {
    let mut result = Vec::new();
    for spec in WORLD_TABLES {
        for mut row in crate::sql_extract::rows(db, spec)? {
            if spec.name == "landblock_instance"
                && let Some(v) = row.get_mut("is_link_child")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "recipe_mod"
                && let Some(v) = row.get_mut("executes_on_success")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "recipe_mod"
                && let Some(v) = row.get_mut("unknown_7")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "recipe_mods_bool"
                && let Some(v) = row.get_mut("value")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "recipe_requirements_bool"
                && let Some(v) = row.get_mut("value")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "spell"
                && let Some(v) = row.get_mut("non_tracking")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "treasure_wielded"
                && let Some(v) = row.get_mut("set_start")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "treasure_wielded"
                && let Some(v) = row.get_mut("has_sub_set")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            if spec.name == "treasure_wielded"
                && let Some(v) = row.get_mut("continues_previous_set")
                && !v.is_null()
            {
                *v = match v.as_u64() {
                    Some(0) => false.into(),
                    Some(1) => true.into(),
                    _ => return Err(SqlStagingError::Extraction("invalid world boolean".into())),
                };
            }
            let value = serde_json::Value::Object(row);
            let record = match spec.name {
                "cook_book" => WorldRecordV1::CookBook(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "encounter" => WorldRecordV1::Encounter(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "event" => WorldRecordV1::Event(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "house_portal" => WorldRecordV1::HousePortal(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "landblock_instance" => WorldRecordV1::LandblockInstance(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "landblock_instance_link" => WorldRecordV1::LandblockInstanceLink(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "points_of_interest" => WorldRecordV1::PointsOfInterest(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "quest" => WorldRecordV1::Quest(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe" => WorldRecordV1::Recipe(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mod" => WorldRecordV1::RecipeMod(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mods_bool" => WorldRecordV1::RecipeModsBool(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mods_d_i_d" => WorldRecordV1::RecipeModsDID(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mods_float" => WorldRecordV1::RecipeModsFloat(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mods_i_i_d" => WorldRecordV1::RecipeModsIID(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mods_int" => WorldRecordV1::RecipeModsInt(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_mods_string" => WorldRecordV1::RecipeModsString(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_requirements_bool" => WorldRecordV1::RecipeRequirementsBool(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_requirements_d_i_d" => WorldRecordV1::RecipeRequirementsDID(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_requirements_float" => WorldRecordV1::RecipeRequirementsFloat(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_requirements_i_i_d" => WorldRecordV1::RecipeRequirementsIID(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_requirements_int" => WorldRecordV1::RecipeRequirementsInt(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "recipe_requirements_string" => WorldRecordV1::RecipeRequirementsString(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "spell" => WorldRecordV1::Spell(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "treasure_death" => WorldRecordV1::TreasureDeath(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "treasure_gem_count" => WorldRecordV1::TreasureGemCount(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "treasure_material_base" => WorldRecordV1::TreasureMaterialBase(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "treasure_material_color" => WorldRecordV1::TreasureMaterialColor(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "treasure_material_groups" => WorldRecordV1::TreasureMaterialGroups(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "treasure_wielded" => WorldRecordV1::TreasureWielded(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                "version" => WorldRecordV1::Version(
                    serde_json::from_value(value)
                        .map_err(|e| SqlStagingError::Extraction(e.to_string()))?,
                ),
                _ => return Err(SqlStagingError::Extraction("unmapped world table".into())),
            };
            record.validate().map_err(SqlStagingError::Extraction)?;
            if result.len() >= 3000000 {
                return Err(SqlStagingError::Extraction("world row limit".into()));
            }
            result.push(record);
        }
    }
    Ok(result)
}
