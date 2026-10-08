//! Schema-preserving form for already present TOML fields. Domain codecs still
//! validate the complete document before any save or candidate build.
use eframe::egui;
use std::collections::BTreeMap;

fn label(name: &str) -> String {
    name.replace('_', " ")
}

pub(crate) fn edit(
    ui: &mut egui::Ui,
    value: &mut toml::Value,
    path: &str,
    drafts: &mut BTreeMap<String, String>,
) -> bool {
    match value {
        toml::Value::Table(table) => {
            let mut changed = false;
            for (key, child) in table.iter_mut() {
                let child_path = format!("{path}/{key}");
                if matches!(child, toml::Value::Table(_) | toml::Value::Array(_)) {
                    ui.collapsing(label(key), |ui| {
                        changed |= edit(ui, child, &child_path, drafts);
                    });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(label(key));
                        changed |= edit(ui, child, &child_path, drafts);
                    });
                }
            }
            changed
        }
        toml::Value::Array(array) => {
            let mut changed = false;
            let mut remove = None;
            let mut duplicate = None;
            let mut up = None;
            let mut down = None;
            let length = array.len();
            for (index, child) in array.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Row {}", index + 1));
                    if ui.small_button("↑").clicked() && index > 0 {
                        up = Some(index);
                    }
                    if ui.small_button("↓").clicked() && index + 1 < length {
                        down = Some(index);
                    }
                    if ui.small_button("Duplicate").clicked() {
                        duplicate = Some(index);
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
                ui.indent((path, index), |ui| {
                    changed |= edit(ui, child, &format!("{path}/{index}"), drafts);
                });
            }
            if let Some(index) = up {
                array.swap(index, index - 1);
                changed = true;
            }
            if let Some(index) = down {
                array.swap(index, index + 1);
                changed = true;
            }
            if let Some(index) = duplicate {
                array.insert(index + 1, array[index].clone());
                changed = true;
            }
            if let Some(index) = remove {
                array.remove(index);
                changed = true;
            }
            changed
        }
        toml::Value::String(text) => ui
            .add(egui::TextEdit::singleline(text).desired_width(260.0))
            .changed(),
        toml::Value::Boolean(boolean) => ui.checkbox(boolean, "").changed(),
        toml::Value::Integer(integer) => {
            let draft = drafts
                .entry(path.to_owned())
                .or_insert_with(|| integer.to_string());
            let response = ui.add(egui::TextEdit::singleline(draft).desired_width(135.0));
            if response.lost_focus()
                && let Ok(parsed) = draft.parse::<i64>()
                && parsed != *integer
            {
                *integer = parsed;
                return true;
            }
            false
        }
        toml::Value::Float(number) => {
            let draft = drafts
                .entry(path.to_owned())
                .or_insert_with(|| number.to_string());
            let response = ui.add(egui::TextEdit::singleline(draft).desired_width(135.0));
            if response.lost_focus()
                && let Ok(parsed) = draft.parse::<f64>()
                && parsed.is_finite()
                && parsed != *number
            {
                *number = parsed;
                return true;
            }
            false
        }
        toml::Value::Datetime(datetime) => {
            ui.monospace(datetime.to_string());
            false
        }
    }
}

/// Expose absent optional text fields in quests and recipe rows.
/// Other field creation remains available through validated TOML source.
pub(crate) fn optional_world_messages(
    ui: &mut egui::Ui,
    namespace: u16,
    value: &mut toml::Value,
) -> bool {
    let (variant, fields): (&str, &[&str]) = match namespace {
        23 => ("Quest", &["message"]),
        24 => (
            "Recipe",
            &[
                "success_message",
                "fail_message",
                "success_destroy_source_message",
                "success_destroy_target_message",
                "fail_destroy_source_message",
                "fail_destroy_target_message",
            ],
        ),
        31 => ("RecipeModsString", &["value"]),
        32 => ("RecipeRequirementsBool", &["message"]),
        33 => ("RecipeRequirementsDID", &["message"]),
        34 => ("RecipeRequirementsFloat", &["message"]),
        35 => ("RecipeRequirementsIID", &["message"]),
        36 => ("RecipeRequirementsInt", &["message"]),
        37 => ("RecipeRequirementsString", &["value", "message"]),
        _ => return false,
    };
    let Some(table) = value
        .as_table_mut()
        .and_then(|root| root.get_mut(variant))
        .and_then(toml::Value::as_table_mut)
    else {
        return false;
    };
    let mut changed = false;
    for &field in fields {
        if !table.contains_key(field) && ui.button(format!("Add {}", label(field))).clicked() {
            table.insert(field.into(), toml::Value::String(String::new()));
            changed = true;
        }
    }
    changed
}
