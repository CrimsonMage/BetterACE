//! Native Rust EmoteScript boundary. Explicit fields preserve all native action values.
use crate::emote_script_schema as schema;
use bace_content::{Emote, EmoteAction};
use serde_json::{Map, Value};

/// Compile event/action syntax, named fields, positional fields and indented branches.
/// Parsing is transactional: errors include a line number and return no partial result.
pub fn import_emote_script(source: &str) -> Result<Vec<Emote>, String> {
    if source.len() > 1024 * 1024 {
        return Err("EmoteScript exceeds 1 MiB".into());
    }
    let mut sets: Vec<Emote> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut actions = 0;
    let mut linked = std::collections::BTreeSet::new();
    for (line_number, line) in source.lines().enumerate() {
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') || text.starts_with("//") {
            continue;
        }
        if line_number >= 10_000 {
            return Err("EmoteScript exceeds 10,000 lines".into());
        }
        let indent = line
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum::<usize>();
        if indent > 256 {
            return Err(format!(
                "Line {}: nesting exceeds 64 levels",
                line_number + 1
            ));
        }
        let result = (|| {
            let action = text.starts_with('-');
            let mut text = text.trim_start_matches('-').trim();
            let mut prefix = Map::new();
            loop {
                let field = if text.starts_with("Delay:") {
                    Some("delay")
                } else if text.starts_with("Extent:") {
                    Some("extent")
                } else {
                    None
                };
                let Some(field) = field else {
                    break;
                };
                let (number, rest) = text
                    .split_once(',')
                    .ok_or("Delay/Extent prefix needs a following action")?;
                prefix.insert(
                    field.into(),
                    schema::parse_value(
                        "Float",
                        number.split_once(':').ok_or("Missing prefix value")?.1,
                    )?,
                );
                text = rest.trim();
            }
            let (command, parameters) = text.split_once(':').unwrap_or((text, ""));
            let command = command.trim();
            let is_action = action || schema::enum_id("EmoteCategory", command).is_none();
            if is_action {
                while stack.last().is_some_and(|(depth, _)| *depth >= indent) {
                    stack.pop();
                }
                let &(_, parent) = stack
                    .last()
                    .ok_or("Action requires an indented event set")?;
                let kind = schema::enum_id("EmoteType", command)
                    .ok_or_else(|| format!("Unknown action {command}"))?;
                let mut value = serde_json::to_value(EmoteAction {
                    r#type: kind,
                    extent: if command.eq_ignore_ascii_case("Say") {
                        0.0
                    } else {
                        1.0
                    },
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?;
                let table = value.as_object_mut().ok_or("Action is not an object")?;
                apply_fields(table, "action", command, parameters)?;
                table.extend(prefix);
                let mut action: EmoteAction =
                    serde_json::from_value(value).map_err(|e| e.to_string())?;
                if schema::metadata("branch", "action", command).is_some()
                    && action.message.is_none()
                {
                    action.message = Some(format!("studio_branch_{}", line_number + 1));
                }
                sets[parent].actions.push(action);
                actions += 1;
                if actions > 10_000 {
                    return Err("Action limit exceeds 10,000".into());
                }
            } else {
                while stack.last().is_some_and(|(depth, _)| *depth >= indent) {
                    stack.pop();
                }
                let category =
                    schema::enum_id("EmoteCategory", command).ok_or("Unknown emote category")?;
                let mut value = serde_json::to_value(Emote {
                    category: i32::try_from(category)
                        .map_err(|_| "Emote category exceeds signed 32-bit range")?,
                    probability: 1.0,
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?;
                apply_fields(
                    value.as_object_mut().ok_or("Event is not an object")?,
                    "set",
                    command,
                    parameters,
                )?;
                let mut set: Emote = serde_json::from_value(value).map_err(|e| e.to_string())?;
                if let Some(&(_, parent)) = stack.last() {
                    let action_index = sets[parent]
                        .actions
                        .len()
                        .checked_sub(1)
                        .ok_or("Nested event requires a preceding branch action")?;
                    let action = sets[parent]
                        .actions
                        .last_mut()
                        .ok_or("Nested event requires a preceding branch action")?;
                    let name = schema::enum_name("EmoteType", i64::from(action.r#type));
                    let allowed = schema::metadata("branch", "action", &name)
                        .ok_or("Preceding action has no branches")?;
                    if !allowed
                        .split(',')
                        .any(|n| schema::enum_id("EmoteCategory", n) == Some(category))
                    {
                        return Err(format!("{command} is not a branch of {name}"));
                    }
                    // Quest/event names are also runtime query inputs. ACE strips an
                    // @ suffix when looking them up; the full key identifies branches.
                    if linked.insert((parent, action_index))
                        && (name.contains("Quest") || name.contains("Event"))
                    {
                        let message = action
                            .message
                            .as_mut()
                            .ok_or("Branch query requires a name")?;
                        if !message.contains('@') {
                            message.push_str(&format!("@studio_{}", line_number + 1));
                        }
                    }
                    if set.quest.is_some() && set.quest != action.message {
                        return Err(
                            "Nested branch Quest must match its parent action Message".into()
                        );
                    }
                    set.quest = action.message.clone();
                } else if indent != 0 {
                    return Err("Root events must start at column one".into());
                }
                stack.push((indent, sets.len()));
                sets.push(set);
            }
            Ok(())
        })();
        if let Err(e) = result {
            return Err(format!("Line {}: {e}", line_number + 1));
        }
    }
    Ok(sets)
}
fn apply_fields(
    table: &mut Map<String, Value>,
    family: &str,
    command: &str,
    parameters: &str,
) -> Result<(), String> {
    if parameters.trim().is_empty() {
        return Ok(());
    }
    let mut seen = std::collections::BTreeSet::new();
    for token in tokens(parameters, family)? {
        if let Some((field, value)) = token.split_once(':')
            && let Some((name, typ)) = schema::field(family, field.trim())
        {
            let key = schema::key(name);
            if !seen.insert(key.clone()) {
                return Err(format!("Duplicate field {name}"));
            }
            table.insert(key, schema::parse_value(typ, value)?);
            continue;
        }
        let defaults = schema::metadata("default", family, command)
            .ok_or_else(|| format!("{command} requires explicit named fields"))?;
        let fields: Vec<_> = defaults.split(',').collect();
        let values = if fields.len() == 1 {
            vec![token.trim()]
        } else {
            token.split(", ").collect::<Vec<_>>()
        };
        if values.len() > fields.len() {
            return Err("Too many positional fields; use named fields".into());
        }
        for (field, value) in fields.into_iter().zip(values) {
            if matches!(field, "Position" | "OriginAngles" | "Angles") {
                for (key, value) in crate::emote_script_position::fields(field, value)? {
                    if !seen.insert(key.clone()) {
                        return Err(format!("Duplicate field {key}"));
                    }
                    table.insert(key, value);
                }
                continue;
            }
            if matches!(field, "Range" | "Range64" | "RangeFloat") {
                let (minimum, maximum) = match field {
                    "Range64" => ("min64", "max64"),
                    "RangeFloat" => ("min_dbl", "max_dbl"),
                    _ => ("min", "max"),
                };
                let typ = if field == "RangeFloat" {
                    "Double"
                } else {
                    "Int64"
                };
                let split = value
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| *c == '-')
                    .map(|(i, _)| (&value[..i], &value[i + 1..]));
                let pairs = if let Some((a, b)) = split {
                    vec![(minimum, a), (maximum, b)]
                } else {
                    vec![(
                        if command.to_ascii_lowercase().starts_with("award") {
                            maximum
                        } else {
                            minimum
                        },
                        value,
                    )]
                };
                for (key, value) in pairs {
                    if !seen.insert(key.to_string()) {
                        return Err(format!("Duplicate field {key}"));
                    }
                    table.insert(key.into(), schema::parse_value(typ, value)?);
                }
                continue;
            }

            let (name, typ) = match field {
                "CharacterTitle" => ("Amount", "CharacterTitle"),
                "ContractId" => ("Stat", "ContractId"),
                "SkillStat" => ("Stat", "Skill"),
                "PropertyAttributeStat" => ("Stat", "PropertyAttribute"),
                "PropertyAttribute2ndStat" => ("Stat", "PropertyAttribute2nd"),
                "PropertyBoolStat" => ("Stat", "PropertyBool"),
                "PropertyFloatStat" => ("Stat", "PropertyFloat"),
                "PropertyIntStat" => ("Stat", "PropertyInt"),
                "PropertyInt64Stat" => ("Stat", "PropertyInt64"),
                "PropertyStringStat" => ("Stat", "PropertyString"),
                _ => schema::field(family, field)
                    .ok_or_else(|| format!("Use explicit fields instead of {field} shorthand"))?,
            };
            let key = schema::key(name);
            if !seen.insert(key.clone()) {
                return Err(format!("Duplicate field {name}"));
            }
            table.insert(key, schema::parse_value(typ, value)?);
        }
    }
    Ok(())
}
fn tokens<'a>(text: &'a str, family: &str) -> Result<Vec<&'a str>, String> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut quote = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quote {
            escaped = true;
            continue;
        }
        if c == '"' {
            quote = !quote;
            continue;
        }
        if c == ',' && !quote {
            let suffix = text[i + 1..].trim_start();
            if suffix
                .split_once(':')
                .is_some_and(|(name, _)| schema::field(family, name.trim()).is_some())
            {
                out.push(text[start..i].trim());
                start = i + 1;
            }
        }
    }
    if quote {
        return Err("Unterminated quoted string".into());
    }
    out.push(text[start..].trim());
    Ok(out)
}

/// Emit flat, explicitly linked event sets. Relational IDs remain in native TOML.
pub fn export_emote_script(emotes: &[Emote]) -> Result<String, String> {
    let mut out =
        String::from("# BetterACE EmoteScript · native TOML retains relational metadata\n");
    for set in emotes {
        out.push_str(&schema::enum_name("EmoteCategory", i64::from(set.category)));
        out.push(':');
        fields(
            &mut out,
            "set",
            serde_json::to_value(set).map_err(|e| e.to_string())?,
        )?;
        out.push('\n');
        for action in &set.actions {
            out.push_str("    - ");
            out.push_str(&schema::enum_name("EmoteType", i64::from(action.r#type)));
            out.push(':');
            fields(
                &mut out,
                "action",
                serde_json::to_value(action).map_err(|e| e.to_string())?,
            )?;
            out.push('\n');
        }
        out.push('\n');
        if out.len() > 1024 * 1024 {
            return Err("EmoteScript exceeds 1 MiB".into());
        }
    }
    Ok(out)
}
fn fields(out: &mut String, family: &str, value: Value) -> Result<(), String> {
    let mut first = true;
    for row in schema::rows().filter(|r| r.len() == 4 && r[0] == "field" && r[1] == family) {
        if let Some(v) = value.get(schema::key(row[2]))
            && !v.is_null()
        {
            if !first {
                out.push(',');
            }
            first = false;
            out.push(' ');
            out.push_str(row[2]);
            out.push_str(": ");
            out.push_str(&serde_json::to_string(v).map_err(|e| e.to_string())?);
        }
    }
    Ok(())
}
