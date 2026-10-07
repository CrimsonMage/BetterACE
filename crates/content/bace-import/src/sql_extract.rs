use crate::{
    SqlStagingError, StagedWorld, StagingManifest,
    mariadb::IsolatedMariaDb,
    sql_specs::{TABLES, TableSpec},
};
use bace_content::{ContentLimits, WeenieTemplate};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn error(message: impl std::fmt::Display) -> SqlStagingError {
    SqlStagingError::Extraction(message.to_string())
}
fn identifier(value: &str) -> Result<String, SqlStagingError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(error("unexpected SQL identifier"));
    }
    Ok(format!("`{value}`"))
}
fn rows(
    db: &IsolatedMariaDb,
    spec: &TableSpec,
) -> Result<Vec<Map<String, Value>>, SqlStagingError> {
    let actual=db.query(&format!("SELECT COLUMN_NAME FROM information_schema.columns WHERE TABLE_SCHEMA='ace_world' AND TABLE_NAME='{}' ORDER BY ORDINAL_POSITION",spec.name))?;
    let actual: BTreeSet<_> = actual.lines().collect();
    let expected: BTreeSet<_> = spec.fields.iter().map(|(column, _)| *column).collect();
    if actual != expected {
        return Err(error(format!(
            "unsupported schema columns for {}",
            spec.name
        )));
    }
    let fields = spec
        .fields
        .iter()
        .map(|(column, native)| {
            let column = identifier(column)?;
            let expression = if (spec.native == "bools" && *native == "value")
                || matches!(*native, "try_to_bond" | "ignore_author" | "display")
            {
                format!("CAST({column} AS UNSIGNED)")
            } else {
                column
            };
            Ok(format!("'{native}',{expression}"))
        })
        .collect::<Result<Vec<_>, SqlStagingError>>()?
        .join(",");
    let ordering = spec
        .order
        .split(',')
        .map(identifier)
        .collect::<Result<Vec<_>, _>>()?
        .join(",");
    let output = db.query(&format!(
        "SELECT JSON_OBJECT({fields}) FROM ace_world.{} ORDER BY {ordering}",
        identifier(spec.name)?
    ))?;
    output
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|e| error(format!("{} JSON extraction error: {e}", spec.name)))
        })
        .collect()
}
fn numeric(row: &Map<String, Value>, key: &str) -> Result<u32, SqlStagingError> {
    let value = row
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| error(format!("invalid {key}")))?;
    u32::try_from(value).map_err(error)
}
fn properties(template: &mut Value) -> Result<&mut Map<String, Value>, SqlStagingError> {
    template
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| error("missing native properties"))
}
fn collection<'a>(
    properties: &'a mut Map<String, Value>,
    name: &str,
) -> Result<&'a mut Vec<Value>, SqlStagingError> {
    properties
        .entry(name.to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| error("native collection shape mismatch"))
}
fn boolean(value: &Value) -> Result<Value, SqlStagingError> {
    match value.as_u64() {
        Some(0) => Ok(false.into()),
        Some(1) => Ok(true.into()),
        _ => Err(error("SQL boolean must be 0 or 1")),
    }
}

pub(crate) fn extract(
    db: &IsolatedMariaDb,
    source_sha256: String,
) -> Result<StagedWorld, SqlStagingError> {
    let other_schemas = db.query("SELECT SCHEMA_NAME FROM information_schema.schemata WHERE SCHEMA_NAME NOT IN ('mysql','information_schema','performance_schema','sys','ace_world')")?;
    if !other_schemas.trim().is_empty() {
        return Err(error("SQL created unsupported databases outside ace_world"));
    }
    let mut counts = BTreeMap::new();
    let tables=db.query("SELECT TABLE_NAME FROM information_schema.tables WHERE TABLE_SCHEMA='ace_world' ORDER BY TABLE_NAME")?;
    for table in tables.lines() {
        let count = db
            .query(&format!(
                "SELECT COUNT(*) FROM ace_world.{}",
                identifier(table)?
            ))?
            .trim()
            .parse::<u64>()
            .map_err(error)?;
        counts.insert(table.to_owned(), count);
    }
    let unsupported: Vec<_> = counts
        .iter()
        .filter(|(table, count)| **count > 0 && !TABLES.iter().any(|s| s.name == table.as_str()))
        .map(|(table, _)| table.clone())
        .collect();
    if !unsupported.is_empty() {
        return Err(SqlStagingError::UnsupportedTables(unsupported));
    }
    let mut templates: BTreeMap<u32, Value> = BTreeMap::new();
    for mut row in rows(db, &TABLES[0])? {
        let id = numeric(&row, "weenie_id")?;
        row.insert("schema_version".into(), 1.into());
        row.insert("properties".into(), json!({}));
        templates.insert(id, row.into());
    }
    if templates.is_empty() {
        return Err(error("SQL import contains no weenies"));
    }
    let mut emotes: BTreeMap<u32, (u32, usize)> = BTreeMap::new();
    for spec in &TABLES[1..] {
        for mut row in rows(db, spec)? {
            let owner = if spec.native == "actions" {
                let parent = numeric(&row, "emote_id")?;
                emotes
                    .get(&parent)
                    .ok_or_else(|| error("orphan emote action"))?
                    .0
            } else {
                numeric(&row, "object_id")?
            };
            let template = templates
                .get_mut(&owner)
                .ok_or_else(|| error(format!("orphan property owner {owner}")))?;
            let properties = properties(template)?;
            let row_id = numeric(&row, "database_record_id")?;
            let emote_parent = row.get("emote_id").and_then(Value::as_u64);
            let key = spec
                .fields
                .iter()
                .find(|(column, _)| *column == spec.key)
                .and_then(|(_, native)| row.remove(*native));
            row.remove("object_id");
            row.remove("emote_id");
            if !matches!(
                spec.native,
                "emotes" | "actions" | "generators" | "create_list"
            ) {
                row.remove("database_record_id");
            }
            for field in ["try_to_bond", "ignore_author", "display"] {
                if let Some(value) = row.get(field)
                    && !value.is_null()
                {
                    let value = boolean(value)?;
                    row.insert(field.into(), value);
                }
            }
            if spec.native == "book" {
                properties.insert("book".into(), row.into());
                continue;
            }
            if spec.native == "actions" {
                let parent =
                    u32::try_from(emote_parent.ok_or_else(|| error("missing emote parent"))?)
                        .map_err(error)?;
                let index = emotes[&parent].1;
                let emote = collection(properties, "emotes")?
                    .get_mut(index)
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| error("missing emote"))?;
                collection(emote, "actions")?.push(row.into());
                continue;
            }
            let values = collection(properties, spec.native)?;
            if spec.native == "emotes" {
                emotes.insert(row_id, (owner, values.len()));
            }
            let output = if spec.native == "event_filter" {
                row.remove("event").ok_or_else(|| error("missing event"))?
            } else if let Some(key) = key {
                let value = if let Some(value) = row.remove("value") {
                    if spec.native == "bools" {
                        boolean(&value)?
                    } else {
                        value
                    }
                } else if spec.native == "spell_book" {
                    row.remove("probability")
                        .ok_or_else(|| error("missing spell probability"))?
                } else {
                    row.into()
                };
                json!({"id":key,"value":value})
            } else {
                row.into()
            };
            values.push(output);
        }
    }
    let mut result = Vec::with_capacity(templates.len());
    for value in templates.into_values() {
        let mut template: WeenieTemplate = serde_json::from_value(value).map_err(error)?;
        template.validate(ContentLimits::default())?;
        template.canonicalize();
        result.push(template);
    }
    let manifest = StagingManifest {
        source_sha256,
        source_release: "operator-supplied-weenie-sql".into(),
        upstream_pin: "47edade3bd3f6044b676d4eb877c4965c7eda62b".into(),
        table_row_counts: counts,
        unsupported_tables: vec![],
    };
    crate::staging_inventory::validate(&manifest, &result)?;
    Ok(StagedWorld {
        manifest,
        weenies: result,
    })
}
