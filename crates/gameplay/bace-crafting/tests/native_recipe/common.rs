use bace_content::RecipeRowV1;
use bace_crafting::*;
use std::collections::BTreeMap;
pub fn source() -> RecipeRowV1 {
    RecipeRowV1 {
        id: 1,
        unknown_1: 0,
        skill: 28,
        difficulty: 100,
        salvage_type: 1,
        success_w_c_i_d: 0,
        success_amount: 0,
        success_message: Some("success".into()),
        fail_w_c_i_d: 0,
        fail_amount: 0,
        fail_message: None,
        success_destroy_source_chance: 0.0,
        success_destroy_source_amount: 0,
        success_destroy_source_message: None,
        success_destroy_target_chance: 0.0,
        success_destroy_target_amount: 0,
        success_destroy_target_message: None,
        fail_destroy_source_chance: 0.0,
        fail_destroy_source_amount: 0,
        fail_destroy_source_message: None,
        fail_destroy_target_chance: 0.0,
        fail_destroy_target_amount: 0,
        fail_destroy_target_message: None,
        data_id: 0,
        last_modified: String::new(),
    }
}
pub fn context() -> CraftContext {
    CraftContext {
        actor: 1,
        actor_revision: 1,
        character_random_id: [1; 16],
        operation_id: [2; 16],
        busy: false,
        peace_mode: true,
        chance: ChanceInput {
            skill: 1000,
            trained: true,
            lum_craft: 0,
            tool_workmanship: 10.0,
            target_workmanship: 10.0,
            material: 61,
            times_tinkered: 0,
            imbue: false,
            imbue_augmentation: false,
            foolproof: true,
        },
        properties: properties(1, 0, 123),
    }
}
pub fn item(id: u32) -> CraftItem {
    CraftItem {
        id,
        owner: 1,
        revision: 1,
        stack: 1,
        equipped: false,
        in_trade: false,
        reserved: false,
        times_tinkered: 0,
        tinker_log: vec![],
        properties: properties(id, 0, 123),
    }
}
pub fn properties(id: u32, seed: u32, stat: u32) -> BTreeMap<PropertyKey, PropertyValue> {
    let mut properties = BTreeMap::from([(
        PropertyKey {
            kind: PropertyKind::String,
            id: 1,
        },
        PropertyValue::String(format!("item{id}")),
    )]);
    if seed > 0 {
        let n = id * 10;
        for value in [
            PropertyValue::Int(n as i32),
            PropertyValue::Float(f64::from(n) + 0.125),
            PropertyValue::Bool(true),
            PropertyValue::String(format!("saved{n}")),
            PropertyValue::DataId(n),
            PropertyValue::InstanceId(n),
        ] {
            properties.insert(
                PropertyKey {
                    kind: value.kind(),
                    id: stat,
                },
                value,
            );
        }
    }
    properties
}
pub fn parse(text: &str) -> BTreeMap<PropertyKey, PropertyValue> {
    text.split(';')
        .filter(|s| !s.is_empty())
        .map(|row| {
            let fields: Vec<_> = row.split(':').collect();
            let raw = fields[2];
            let value = match fields[0] {
                "I" => PropertyValue::Int(raw.parse().unwrap()),
                "F" => PropertyValue::Float(f64::from_bits(u64::from_str_radix(raw, 16).unwrap())),
                "B" => PropertyValue::Bool(raw == "1"),
                "D" => PropertyValue::DataId(raw.parse().unwrap()),
                "G" => PropertyValue::InstanceId(raw.parse().unwrap()),
                "S" => PropertyValue::String(raw.to_owned()),
                "P" => PropertyValue::SpellBook(raw == "1"),
                _ => panic!("fixture"),
            };
            (
                PropertyKey {
                    kind: value.kind(),
                    id: fields[1].parse().unwrap(),
                },
                value,
            )
        })
        .collect()
}
