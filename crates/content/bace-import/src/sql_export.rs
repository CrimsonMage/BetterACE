//! Legacy file export only, never production SQL/connections. Mappings reuse
//! the pinned official WorldBase.sql extraction inventory. Generated SQL must
//! be reviewed before loading into an external ACE database.
use crate::{
    ImportError,
    sql_specs::{TABLES, TableSpec},
};
use bace_content::{ContentLimits, WeenieTemplate};
use serde_json::{Map, Value};

pub fn export_weenie_sql(template: &WeenieTemplate) -> Result<String, ImportError> {
    template.validate(ContentLimits::default())?;
    check_order(
        template
            .properties
            .book_pages
            .iter()
            .map(|p| p.legacy_page_id),
        "page IDs",
    )?;
    for emote in &template.properties.emotes {
        check_order(
            emote.actions.iter().map(|a| a.legacy_order),
            "emote action order",
        )?;
    }
    if template.properties.authoring_metadata.is_some() {
        return Err(ImportError::Unsupported(
            "authoring_metadata has no official SQL column; preserve native TOML".into(),
        ));
    }
    if template
        .properties
        .emotes
        .iter()
        .any(|e| e.legacy_category_key.is_some())
        || template
            .properties
            .generators
            .iter()
            .any(|g| g.legacy_slot.is_some())
    {
        return Err(ImportError::Unsupported(
            "Lifestoned group/slot metadata has no official SQL column".into(),
        ));
    }
    let root = serde_json::to_value(template)?;
    let props = &root["properties"];
    let mut out = String::from(
        "-- BetterACE legacy weenie export; official ACE schema 47edade3bd3f6044b676d4eb877c4965c7eda62b\n-- INSERT only: existing IDs cause an error. Native TOML is the authoring source.\nSET NAMES utf8mb4;\nSTART TRANSACTION;\n",
    );
    emit(
        &mut out,
        &TABLES[0],
        root.as_object().ok_or_else(|| unsupported("native root"))?,
        None,
    )?;
    for spec in TABLES.iter().skip(1).filter(|s| s.native != "actions") {
        let value = &props[spec.native];
        if value.is_null() {
            continue;
        }
        let rows = if spec.native == "book" {
            vec![value]
        } else {
            value
                .as_array()
                .map(|v| v.iter().collect())
                .unwrap_or_default()
        };
        for (index, value) in rows.into_iter().enumerate() {
            let mut row = Map::new();
            row.insert("object_id".into(), template.weenie_id.into());
            if !spec.key.is_empty() {
                let native_key = spec
                    .fields
                    .iter()
                    .find(|(sql, _)| *sql == spec.key)
                    .map(|(_, native)| *native)
                    .ok_or_else(|| unsupported("dictionary key"))?;
                row.insert(native_key.into(), value["id"].clone());
                if let Some(fields) = value["value"].as_object() {
                    row.extend(fields.clone());
                } else {
                    row.insert(
                        if spec.native == "spell_book" {
                            "probability"
                        } else {
                            "value"
                        }
                        .into(),
                        value["value"].clone(),
                    );
                }
            } else if spec.native == "event_filter" {
                row.insert("event".into(), value.clone());
            } else if let Some(fields) = value.as_object() {
                row.extend(fields.clone());
            }
            if spec.native == "book_pages" && row.get("legacy_page_id").is_none_or(Value::is_null) {
                row.insert("legacy_page_id".into(), (index as u64).into());
            }
            row.remove("legacy_category_key");
            row.remove("legacy_slot");
            row.remove("actions");
            emit(&mut out, spec, &row, None)?;
            if spec.native == "emotes" {
                out.push_str("SET @bace_emote_id = LAST_INSERT_ID();\n");
                if let Some(actions) = value["actions"].as_array() {
                    let action_spec = TABLES
                        .iter()
                        .find(|s| s.native == "actions")
                        .ok_or_else(|| unsupported("action schema"))?;
                    for (index, action) in actions.iter().enumerate() {
                        let mut row = action
                            .as_object()
                            .ok_or_else(|| unsupported("action"))?
                            .clone();
                        if row.get("legacy_order").is_none_or(Value::is_null) {
                            row.insert("legacy_order".into(), (index as u64).into());
                        }
                        emit(&mut out, action_spec, &row, Some("@bace_emote_id"))?;
                    }
                }
            }
        }
    }
    out.push_str("COMMIT;\n");
    if out.len() > 16 * 1024 * 1024 {
        return Err(ImportError::Limit);
    }
    Ok(out)
}

fn unsupported(message: &str) -> ImportError {
    ImportError::Unsupported(message.into())
}

fn check_order(values: impl Iterator<Item = Option<u32>>, name: &str) -> Result<(), ImportError> {
    let mut previous = None;
    for (index, value) in values.enumerate() {
        let value = value.unwrap_or(index as u32);
        if previous.is_some_and(|p| p >= value) {
            return Err(unsupported(&format!(
                "{name} conflicts with authored sequence; renumber it explicitly before SQL export"
            )));
        }
        previous = Some(value);
    }
    Ok(())
}

fn emit(
    out: &mut String,
    spec: &TableSpec,
    row: &Map<String, Value>,
    parent: Option<&str>,
) -> Result<(), ImportError> {
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for (column, native) in spec.fields {
        // Storage row IDs are assigned by the destination, not authored IDs.
        if *native == "database_record_id" {
            continue;
        }
        if *native == "emote_id" {
            columns.push(format!("`{column}`"));
            values.push(
                parent
                    .ok_or_else(|| unsupported("emote parent"))?
                    .to_string(),
            );
            continue;
        }
        let value = row.get(*native).unwrap_or(&Value::Null);
        if *native == "last_modified" && value.is_null() {
            continue;
        }
        columns.push(format!("`{column}`"));
        values.push(literal(value)?);
    }
    out.push_str(&format!(
        "INSERT INTO `{}` ({}) VALUES ({});\n",
        spec.name,
        columns.join(", "),
        values.join(", ")
    ));
    if out.len() > 16 * 1024 * 1024 {
        return Err(ImportError::Limit);
    }
    Ok(())
}

fn literal(value: &Value) -> Result<String, ImportError> {
    Ok(match value {
        Value::Null => "NULL".into(),
        Value::Bool(v) => if *v { "1" } else { "0" }.into(),
        Value::Number(v) => v.to_string(),
        // Hex UTF-8 literals avoid quoting/backslash/SQL-mode ambiguities and
        // cannot inject additional statements, even for NUL/newline text.
        Value::String(text) => {
            use std::fmt::Write;
            let mut hex = String::with_capacity(text.len() * 2);
            for byte in text.bytes() {
                write!(&mut hex, "{byte:02x}").map_err(|_| unsupported("string encoding"))?;
            }
            format!("CONVERT(X'{hex}' USING utf8mb4)")
        }
        _ => return Err(unsupported("unexpected structured SQL value")),
    })
}
