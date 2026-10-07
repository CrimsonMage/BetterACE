use bace_content::{ContentError, ContentLimits, WeenieTemplate};
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::{enum_names, legacy_names, property_names, strict_json::StrictJson};

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("legacy JSON exceeds 16 MiB source limit")]
    Limit,
    #[error("invalid legacy JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported legacy field or shape: {0}; import aborted without dropping data")]
    Unsupported(String),
    #[error("unknown legacy enum {family} value {value}; supply its numeric value")]
    Enum { family: String, value: String },
    #[error(transparent)]
    Content(#[from] ContentError),
}

/// Detects official Lifestoned/GDLE (`wcid`) and ACE.Entity.Models.Weenie
/// (`WeenieClassId`) shapes. Numeric IDs and pinned enum names are retained;
/// unknown dialects, duplicate fields and lossy integer conversions fail.
pub fn import_weenie_json(source: &str) -> Result<WeenieTemplate, ImportError> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(ImportError::Limit);
    }
    let StrictJson(value) = serde_json::from_str(source)?;
    let Value::Object(mut root) = value else {
        return Err(ImportError::Unsupported(
            "root must be a Weenie object".into(),
        ));
    };
    if root.contains_key("wcid") {
        return crate::lifestoned::convert(root);
    }
    let id = take(&mut root, "WeenieClassId")?;
    let class = take(&mut root, "ClassName")?;
    let kind = enum_value("WeenieType", take(&mut root, "WeenieType")?)?;
    let mut properties = Map::new();
    for (legacy, native, family) in [
        ("PropertiesBool", "bools", "PropertyBool"),
        ("PropertiesDID", "data_ids", "PropertyDataId"),
        ("PropertiesFloat", "floats", "PropertyFloat"),
        ("PropertiesIID", "instance_ids", "PropertyInstanceId"),
        ("PropertiesInt", "ints", "PropertyInt"),
        ("PropertiesInt64", "int64s", "PropertyInt64"),
        ("PropertiesString", "strings", "PropertyString"),
        ("PropertiesPosition", "positions", "PositionType"),
        ("PropertiesSpellBook", "spell_book", "SpellId"),
        ("PropertiesAttribute", "attributes", "PropertyAttribute"),
        (
            "PropertiesAttribute2nd",
            "secondary_attributes",
            "PropertyAttribute2nd",
        ),
        ("PropertiesBodyPart", "body_parts", "CombatBodyPart"),
        ("PropertiesSkill", "skills", "Skill"),
    ] {
        let Some(value) = root.remove(legacy) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let Value::Object(values) = value else {
            return Err(ImportError::Unsupported(legacy.into()));
        };
        let mut records = Vec::with_capacity(values.len());
        for (key, value) in values {
            let id = if family == "CombatBodyPart" && key == "Undefined" {
                Some(-1_i64)
            } else {
                key.parse::<i64>()
                    .ok()
                    .or_else(|| property_names::resolve(family, &key).map(i64::from))
            }
            .ok_or_else(|| ImportError::Enum {
                family: family.into(),
                value: key,
            })?;
            records.push(json!({"id": id, "value": normalize(value)?}));
        }
        properties.insert(native.into(), records.into());
    }
    for (legacy, native) in [
        ("PropertiesAnimPart", "animation_parts"),
        ("PropertiesPalette", "palettes"),
        ("PropertiesTextureMap", "texture_maps"),
        ("PropertiesCreateList", "create_list"),
        ("PropertiesEmote", "emotes"),
        ("PropertiesEventFilter", "event_filter"),
        ("PropertiesGenerator", "generators"),
        ("PropertiesBook", "book"),
        ("PropertiesBookPageData", "book_pages"),
    ] {
        if let Some(value) = root.remove(legacy)
            && !value.is_null()
        {
            properties.insert(native.into(), normalize(value)?);
        }
    }
    if !root.is_empty() {
        return Err(ImportError::Unsupported(
            root.keys().cloned().collect::<Vec<_>>().join(", "),
        ));
    }
    let mut template: WeenieTemplate = serde_json::from_value(json!({
        "schema_version": 1, "weenie_id": id, "class_name": class,
        "weenie_type": kind, "properties": properties,
    }))?;
    template.validate(ContentLimits::default())?;
    template.canonicalize();
    Ok(template)
}

fn take(root: &mut Map<String, Value>, name: &str) -> Result<Value, ImportError> {
    root.remove(name)
        .ok_or_else(|| ImportError::Unsupported(format!("missing {name}")))
}

fn enum_value(family: &str, value: Value) -> Result<Value, ImportError> {
    let Value::String(name) = value else {
        return Ok(value);
    };
    name.parse::<u32>()
        .ok()
        .or_else(|| enum_names::resolve(family, &name))
        .map(Value::from)
        .ok_or_else(|| ImportError::Enum {
            family: family.into(),
            value: name,
        })
}

fn normalize(value: Value) -> Result<Value, ImportError> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(normalize)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::from),
        Value::Object(values) => {
            let mut normalized = Map::new();
            for (name, value) in values {
                // This is an ORM/navigation backreference, not authored data.
                // Reject embedded objects rather than discarding their contents.
                if name == "Object" && value.is_null() {
                    continue;
                }
                let native = legacy_names::field(&name)
                    .ok_or_else(|| ImportError::Unsupported(name.clone()))?;
                let family = match name.as_str() {
                    "DType" => Some("DamageType"),
                    "DestinationType" => Some("DestinationType"),
                    "Category" => Some("EmoteCategory"),
                    "Style" => Some("MotionStance"),
                    "Substyle" | "Motion" => Some("MotionCommand"),
                    "VendorType" => Some("VendorType"),
                    "PScript" => Some("PlayScript"),
                    "Sound" => Some("Sound"),
                    "WhenCreate" => Some("RegenerationType"),
                    "WhereCreate" => Some("RegenLocationType"),
                    "SAC" => Some("SkillAdvancementClass"),
                    "Type" => Some("EmoteType"),
                    _ => None,
                };
                let value = if let Some(family) = family {
                    enum_value(family, value)?
                } else {
                    normalize(value)?
                };
                if normalized.insert(native.into(), value).is_some() {
                    return Err(ImportError::Unsupported(format!(
                        "duplicate normalized field {native}"
                    )));
                }
            }
            Ok(normalized.into())
        }
        other => Ok(other),
    }
}
