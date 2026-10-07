//! Legacy ACE.Entity.Models.Weenie JSON field contract at the official pin.
//! Author-only fields without an ACE JSON representation reject strict export.
use crate::ImportError;
use bace_content::{ContentLimits, WeenieTemplate};
use serde_json::{Map, Value};

pub fn export_weenie_json(template: &WeenieTemplate) -> Result<String, ImportError> {
    template.validate(ContentLimits::default())?;
    if template.last_modified.is_some() || template.properties.authoring_metadata.is_some() {
        return Err(ImportError::Unsupported(
            "ACE JSON has no last_modified or authoring_metadata; retain them in native TOML"
                .into(),
        ));
    }
    let mut root = Map::new();
    root.insert("WeenieClassId".into(), template.weenie_id.into());
    root.insert("ClassName".into(), template.class_name.clone().into());
    root.insert("WeenieType".into(), template.weenie_type.into());
    let Value::Object(mut props) = serde_json::to_value(&template.properties)? else {
        return Err(ImportError::Unsupported("native properties".into()));
    };
    props.remove("authoring_metadata");
    for (native, legacy) in [
        ("bools", "PropertiesBool"),
        ("data_ids", "PropertiesDID"),
        ("floats", "PropertiesFloat"),
        ("instance_ids", "PropertiesIID"),
        ("ints", "PropertiesInt"),
        ("int64s", "PropertiesInt64"),
        ("strings", "PropertiesString"),
        ("positions", "PropertiesPosition"),
        ("spell_book", "PropertiesSpellBook"),
        ("attributes", "PropertiesAttribute"),
        ("secondary_attributes", "PropertiesAttribute2nd"),
        ("body_parts", "PropertiesBodyPart"),
        ("skills", "PropertiesSkill"),
    ] {
        let rows = props.remove(native).unwrap_or(Value::Null);
        let mut dictionary = Map::new();
        if let Some(rows) = rows.as_array() {
            for row in rows {
                dictionary.insert(row["id"].to_string(), denormalize(row["value"].clone())?);
            }
        }
        root.insert(legacy.into(), dictionary.into());
    }
    for (native, legacy) in [
        ("animation_parts", "PropertiesAnimPart"),
        ("palettes", "PropertiesPalette"),
        ("texture_maps", "PropertiesTextureMap"),
        ("create_list", "PropertiesCreateList"),
        ("emotes", "PropertiesEmote"),
        ("event_filter", "PropertiesEventFilter"),
        ("generators", "PropertiesGenerator"),
        ("book", "PropertiesBook"),
        ("book_pages", "PropertiesBookPageData"),
    ] {
        if let Some(value) = props.remove(native) {
            root.insert(legacy.into(), denormalize(value)?);
        }
    }
    if !props.is_empty() {
        return Err(ImportError::Unsupported(format!(
            "unmapped native fields: {:?}",
            props.keys()
        )));
    }
    let text = serde_json::to_string_pretty(&root)?;
    if text.len() > 16 * 1024 * 1024 {
        return Err(ImportError::Limit);
    }
    Ok(text)
}

fn denormalize(value: Value) -> Result<Value, ImportError> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(denormalize)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::from),
        Value::Object(values) => {
            let mut out = Map::new();
            for (key, value) in values {
                if value.is_null() {
                    continue;
                }
                let legacy = crate::legacy_names::legacy(&key).ok_or_else(|| {
                    ImportError::Unsupported(format!(
                        "{key} has no ACE JSON representation; preserve native TOML"
                    ))
                })?;
                out.insert(legacy.into(), denormalize(value)?);
            }
            Ok(out.into())
        }
        value => Ok(value),
    }
}
