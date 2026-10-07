use eframe::egui::{self, Color32};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub(crate) struct Forms {
    numbers: HashMap<egui::Id, String>,
    invalid: HashSet<egui::Id>,
}

impl Forms {
    pub fn clear(&mut self) {
        self.numbers.clear();
        self.invalid.clear();
    }
    pub fn valid(&self) -> bool {
        self.invalid.is_empty()
    }

    pub fn field(&mut self, ui: &mut egui::Ui, key: &str, value: &mut toml::Value) -> bool {
        let mut changed = false;
        match value {
            toml::Value::Table(table) => {
                for (key, value) in table.iter_mut() {
                    ui.push_id(key, |ui| {
                        changed |= self.field(ui, key, value);
                    });
                }
            }
            toml::Value::Array(array) => {
                // Nested emote actions are ordered; move controls retain order
                // explicitly and never sort authored action sequences.
                ui.label(format!("{key} ({})", array.len()));
                let page_id = ui.make_persistent_id("nested-page");
                let pages = array.len().div_ceil(25).max(1);
                let mut page = ui
                    .data(|data| data.get_temp::<usize>(page_id))
                    .unwrap_or(0)
                    .min(pages - 1);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(page > 0, egui::Button::new("Previous actions"))
                        .clicked()
                    {
                        page -= 1;
                    }
                    ui.label(format!("{} / {pages}", page + 1));
                    if ui
                        .add_enabled(page + 1 < pages, egui::Button::new("Next actions"))
                        .clicked()
                    {
                        page += 1;
                    }
                });
                ui.data_mut(|data| data.insert_temp(page_id, page));
                if key == "actions"
                    && ui
                        .button("Renumber SQL action order to match this list")
                        .clicked()
                {
                    for (index, action) in array.iter_mut().enumerate() {
                        if let Some(table) = action.as_table_mut() {
                            table.insert("legacy_order".into(), toml::Value::Integer(index as i64));
                        }
                    }
                    self.clear();
                    changed = true;
                }
                let mut remove = None;
                let mut swap = None;
                for (index, value) in array.iter_mut().enumerate().skip(page * 25).take(25) {
                    ui.push_id(index, |ui| {
                        egui::CollapsingHeader::new(format!("{} {}", key, index + 1))
                            .id_salt("nested")
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.button("Remove").clicked() {
                                        remove = Some(index);
                                    }
                                    if index > 0 && ui.button("Move up").clicked() {
                                        swap = Some((index, index - 1));
                                    }
                                });
                                changed |= self.field(ui, key, value);
                                changed |= self.optionals(ui, key, value);
                            });
                    });
                }
                if let Some(index) = remove {
                    array.remove(index);
                    self.clear();
                    changed = true;
                }
                if let Some((a, b)) = swap {
                    array.swap(a, b);
                    self.clear();
                    changed = true;
                }
                if key == "actions"
                    && array.len() < 100_000
                    && ui.button("Add action").clicked()
                    && let Ok(value) = crate::form_schema::action()
                {
                    array.push(value);
                    changed = true;
                }
            }
            toml::Value::Boolean(value) => {
                changed = ui.checkbox(value, key).changed();
            }
            toml::Value::String(value) => {
                ui.label(key.replace('_', " "));
                changed = if matches!(key, "message" | "page_text" | "comments") {
                    ui.add(
                        egui::TextEdit::multiline(value)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY)
                            .char_limit(1024 * 1024),
                    )
                    .changed()
                } else {
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .desired_width(350.0)
                            .char_limit(1024 * 1024),
                    )
                    .changed()
                };
            }
            toml::Value::Integer(_) | toml::Value::Float(_) => {
                let id = ui.make_persistent_id("number");
                let text = self.numbers.entry(id).or_insert_with(|| value.to_string());
                ui.horizontal(|ui| {
                    ui.label(key.replace('_', " "));
                    if ui
                        .add(
                            egui::TextEdit::singleline(text)
                                .desired_width(180.0)
                                .char_limit(128),
                        )
                        .changed()
                    {
                        // Parse exact i64 text, never route through f64 sliders.
                        let parsed = match value {
                            toml::Value::Integer(_) => {
                                text.parse::<i64>().ok().map(toml::Value::Integer)
                            }
                            _ => text
                                .parse::<f64>()
                                .ok()
                                .filter(|n| n.is_finite())
                                .map(toml::Value::Float),
                        };
                        if let Some(parsed) = parsed {
                            *value = parsed;
                            self.invalid.remove(&id);
                            changed = true;
                        } else {
                            self.invalid.insert(id);
                        }
                    }
                    if self.invalid.contains(&id) {
                        ui.colored_label(Color32::LIGHT_RED, "Invalid number");
                    }
                });
            }
            toml::Value::Datetime(_) => {
                ui.label("Dates are editable in the TOML source view.");
            }
        }
        changed
    }

    pub fn optionals(&mut self, ui: &mut egui::Ui, kind: &str, value: &mut toml::Value) -> bool {
        let required: &[&str] = match kind {
            "emotes" => &["database_record_id", "category", "probability", "actions"],
            "actions" => &["database_record_id", "type", "delay", "extent"],
            "generators" => &[
                "database_record_id",
                "probability",
                "weenie_class_id",
                "init_create",
                "max_create",
                "when_create",
                "where_create",
            ],
            "book_pages" => &["author_id", "ignore_author"],
            _ => return false,
        };
        let Ok(prototype) = crate::form_schema::optional(kind) else {
            return false;
        };
        let (Some(fields), Some(table)) = (prototype.as_table(), value.as_table_mut()) else {
            return false;
        };
        let mut changed = false;
        ui.menu_button("Optional fields…", |ui| {
            for (key, default) in fields {
                if required.contains(&key.as_str()) {
                    continue;
                }
                let mut present = table.contains_key(key);
                if ui.checkbox(&mut present, key).changed() {
                    if present {
                        table.insert(key.clone(), default.clone());
                    } else {
                        table.remove(key);
                    }
                    changed = true;
                }
            }
        });
        if changed {
            self.clear();
        }
        changed
    }
}

pub(crate) fn labels() -> BTreeMap<String, Vec<(i64, String)>> {
    let mut out: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    for line in include_str!("../data/property-labels.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let columns: Vec<_> = line.split('\t').collect();
        if let [family, id, name] = columns.as_slice()
            && let Ok(id) = id.parse()
        {
            out.entry((*family).into())
                .or_default()
                .push((id, (*name).into()));
        }
    }
    out
}
