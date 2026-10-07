//! Checked mapping of pinned ACE.Adapter.GDLE.Models.LSDWeenie fields.
use crate::{ImportError, class_names, lifestoned_complex, lifestoned_helpers::*};
use bace_content::{ContentLimits, WeenieTemplate};
use serde_json::{Value, json};

pub(crate) fn convert(mut root: Object) -> Result<WeenieTemplate, ImportError> {
    let id = take(&mut root, "wcid")?;
    let kind = take(&mut root, "weenieType")?;
    let last_modified = root.remove("lastModified");
    let mut properties = Object::new();
    for (source, native) in [
        ("boolStats", "bools"),
        ("intStats", "ints"),
        ("int64Stats", "int64s"),
        ("floatStats", "floats"),
        ("didStats", "data_ids"),
        ("iidStats", "instance_ids"),
        ("stringStats", "strings"),
    ] {
        let mut values = Vec::new();
        for record in array(root.remove(source).unwrap_or(Value::Null), source)? {
            let (key, mut value) = key_value(record, source)?;
            if source == "boolStats" {
                value = flag(value)?;
            }
            values.push(json!({"id":key,"value":value}));
        }
        properties.insert(native.into(), values.into());
    }
    if let Some(value) = optional(&mut root, "attributes") {
        attributes(value, &mut properties)?;
    }
    if let Some(value) = optional(&mut root, "body") {
        properties.insert("body_parts".into(), lifestoned_complex::body(value)?);
    }
    if let Some(value) = optional(&mut root, "pageDataList") {
        book(value, &mut properties)?;
    }
    if let Some(value) = optional(&mut root, "createList") {
        properties.insert(
            "create_list".into(),
            array(value, "createList")?
                .into_iter()
                .map(|v| creation(v, false).map(Value::from))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        );
    }
    if let Some(value) = optional(&mut root, "emoteTable") {
        properties.insert("emotes".into(), lifestoned_complex::emotes(value)?);
    }
    if let Some(value) = optional(&mut root, "generatorTable") {
        properties.insert("generators".into(), lifestoned_complex::generators(value)?);
    }
    if let Some(value) = optional(&mut root, "posStats") {
        let mut positions = Vec::new();
        for item in array(value, "posStats")? {
            let (id, value) = key_value(item, "position")?;
            let mut value = object(value, "position value")?;
            let mut position = frame(take(&mut value, "frame")?, true)?;
            position.insert("obj_cell_id".into(), take(&mut value, "objcell_id")?);
            done(&value, "position value")?;
            positions.push(json!({"id":id,"value":position}));
        }
        properties.insert("positions".into(), positions.into());
    }
    if let Some(value) = optional(&mut root, "skills") {
        let mut skills = Vec::new();
        for item in array(value, "skills")? {
            let (id, value) = key_value(item, "skill")?;
            let mut value = object(value, "skill value")?;
            let skill = rename(
                &mut value,
                &[
                    ("level_from_pp", "level_from_pp"),
                    ("last_used_time", "last_used_time"),
                    ("init_level", "init_level"),
                    ("pp", "pp"),
                    ("resistance_of_last_check", "resistance_at_last_check"),
                    ("sac", "sac"),
                ],
            );
            done(&value, "skill value")?;
            skills.push(json!({"id":id,"value":skill}));
        }
        properties.insert("skills".into(), skills.into());
    }
    if let Some(value) = optional(&mut root, "spellbook") {
        let mut spells = Vec::new();
        for item in array(value, "spellbook")? {
            let (id, value) = key_value(item, "spell")?;
            let mut value = object(value, "spell value")?;
            let probability = optional(&mut value, "casting_likelihood").unwrap_or(0.0.into());
            done(&value, "spell value")?;
            spells.push(json!({"id":id,"value":probability}));
        }
        properties.insert("spell_book".into(), spells.into());
    }
    let mut metadata = rename(
        &mut root,
        &[
            ("modifiedBy", "modified_by"),
            ("userChangeSummary", "user_change_summary"),
            ("isDone", "is_done"),
            ("comments", "comments"),
        ],
    );
    if let Some(value) = optional(&mut root, "changelog") {
        metadata.insert("changelog".into(), value);
    }
    if !metadata.is_empty() {
        properties.insert("authoring_metadata".into(), metadata.into());
    }
    done(&root, "Lifestoned root")?;
    let id: u32 = serde_json::from_value(id)?;
    let display = properties
        .get("strings")
        .and_then(Value::as_array)
        .and_then(|values| values.iter().find(|v| v["id"] == 1))
        .and_then(|v| v["value"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| id.to_string());
    let class = class_names::class_name(id, &display);
    let mut result: WeenieTemplate = serde_json::from_value(
        json!({"schema_version":1,"weenie_id":id,"class_name":class,"weenie_type":kind,"last_modified":last_modified,"properties":properties}),
    )?;
    result.validate(ContentLimits::default())?;
    result.canonicalize();
    Ok(result)
}

fn attributes(value: Value, properties: &mut Object) -> Result<(), ImportError> {
    let mut value = object(value, "attributes")?;
    let mut primary = Vec::new();
    let mut secondary = Vec::new();
    for (field, id, is_vital) in [
        ("strength", 1, false),
        ("endurance", 2, false),
        ("quickness", 4, false),
        ("coordination", 3, false),
        ("focus", 5, false),
        ("self", 6, false),
        ("health", 1, true),
        ("stamina", 3, true),
        ("mana", 5, true),
    ] {
        if let Some(attribute) = optional(&mut value, field) {
            let mut attribute = object(attribute, field)?;
            let mut out = rename(
                &mut attribute,
                &[
                    ("cp_spent", "cp_spent"),
                    ("level_from_cp", "level_from_cp"),
                    ("init_level", "init_level"),
                ],
            );
            if is_vital {
                out.extend(rename(&mut attribute, &[("current", "current_level")]));
            }
            done(&attribute, field)?;
            if is_vital {
                secondary.push(json!({"id":id,"value":out}));
            } else {
                primary.push(json!({"id":id,"value":out}));
            }
        }
    }
    done(&value, "attributes")?;
    properties.insert("attributes".into(), primary.into());
    properties.insert("secondary_attributes".into(), secondary.into());
    Ok(())
}

fn book(value: Value, properties: &mut Object) -> Result<(), ImportError> {
    let mut value = object(value, "pageDataList")?;
    let book = rename(
        &mut value,
        &[
            ("maxNumCharsPerPage", "max_num_chars_per_page"),
            ("maxNumPages", "max_num_pages"),
        ],
    );
    let mut pages = Vec::new();
    for page in array(value.remove("pages").unwrap_or(Value::Null), "pages")? {
        let mut page = object(page, "page")?;
        let mut out = rename(
            &mut page,
            &[
                ("authorID", "author_id"),
                ("authorName", "author_name"),
                ("authorAccount", "author_account"),
                ("pageText", "page_text"),
            ],
        );
        if let Some(value) = optional(&mut page, "ignoreAuthor") {
            out.insert("ignore_author".into(), flag(value)?);
        }
        done(&page, "page")?;
        pages.push(out.into());
    }
    done(&value, "pageDataList")?;
    properties.insert("book".into(), book.into());
    properties.insert("book_pages".into(), Value::Array(pages));
    Ok(())
}
