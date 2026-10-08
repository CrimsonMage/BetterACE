use crate::{document, forms::Forms, theme};
use eframe::egui;
use std::{path::PathBuf, sync::mpsc, thread::JoinHandle};

pub(crate) struct LootEditor {
    value: toml::Value,
    forms: Forms,
    saved: toml::Value,
    notice: String,
    job: Option<LootJob>,
}
struct LootJob {
    saved: Option<toml::Value>,
    thread: JoinHandle<()>,
    receiver: mpsc::Receiver<Result<Option<String>, String>>,
}
impl Default for LootEditor {
    fn default() -> Self {
        let value = toml::Value::try_from(bace_content::DeathTreasureV1::default())
            .unwrap_or(toml::Value::Table(Default::default()));
        Self {
            saved: value.clone(),
            value,
            forms: Default::default(),
            notice: String::new(),
            job: None,
        }
    }
}
impl LootEditor {
    pub fn discard(&mut self) {
        self.value = self.saved.clone();
        self.forms.clear();
    }
    pub fn dirty(&self) -> bool {
        self.value != self.saved || !self.forms.valid()
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn start(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        text: Option<String>,
        native_save: bool,
    ) {
        let saved = native_save.then(|| self.value.clone());
        let (sender, receiver) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        match std::thread::Builder::new()
            .name("studio-loot-io".into())
            .spawn(move || {
                let result = if let Some(text) = text {
                    document::write(&path, &text, None).map(|()| None)
                } else {
                    document::read_bounded(&path).and_then(|s| {
                        if s.len() > 64 * 1024 {
                            return Err("Loot profile exceeds 64 KiB".into());
                        }
                        let profile = if path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                        {
                            bace_import::import_loot_json(&s)
                        } else {
                            toml::from_str::<bace_content::DeathTreasureV1>(&s)
                                .map_err(|e| e.to_string())
                        }?;
                        profile.validate()?;
                        toml::to_string_pretty(&profile)
                            .map(Some)
                            .map_err(|e| e.to_string())
                    })
                };
                let _ = sender.send(result);
                ctx.request_repaint();
            }) {
            Ok(thread) => {
                self.job = Some(LootJob {
                    thread,
                    receiver,
                    saved,
                })
            }
            Err(e) => self.notice = e.to_string(),
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.job.as_ref().is_some_and(|j| j.thread.is_finished())
            && let Some(job) = self.job.take()
        {
            let joined = job.thread.join();
            match job.receiver.try_recv() {
                Ok(Ok(Some(text))) if joined.is_ok() => match toml::from_str(&text) {
                    Ok(value) => {
                        self.value = value;
                        self.saved = self.value.clone();
                        self.forms.clear();
                        self.notice = "Opened loot profile.".into();
                    }
                    Err(e) => self.notice = e.to_string(),
                },
                Ok(Ok(None)) if joined.is_ok() => {
                    if let Some(saved) = job.saved {
                        self.saved = saved;
                    }
                    self.notice = "Wrote a new profile file.".into();
                }
                Ok(Err(e)) => self.notice = e,
                _ => self.notice = "Loot worker stopped unexpectedly".into(),
            }
        } else if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        theme::subtitle(
            ui,
            "Author a death-treasure profile, then reference its treasure type from a weenie.",
        );
        ui.add_enabled_ui(!self.busy(), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!self.dirty(), egui::Button::new("Open TOML / JSON…"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Loot profile", &["toml", "json"])
                        .pick_file()
                {
                    self.start(&ctx, path, None, false);
                }
                if ui
                    .add_enabled(self.dirty(), egui::Button::new("Discard changes"))
                    .clicked()
                {
                    self.discard();
                }
                for (extension, label) in [
                    ("toml", "Save TOML as…"),
                    ("json", "Export JSON…"),
                    ("sql", "Export SQL…"),
                ] {
                    if ui
                        .add_enabled(self.forms.valid(), egui::Button::new(label))
                        .clicked()
                    {
                        let result = self
                            .value
                            .clone()
                            .try_into::<bace_content::DeathTreasureV1>()
                            .map_err(|e| e.to_string())
                            .and_then(|p| {
                                p.validate()?;
                                match extension {
                                    "json" => bace_import::export_loot_json(&p),
                                    "sql" => bace_import::export_loot_sql(&p),
                                    _ => toml::to_string_pretty(&p).map_err(|e| e.to_string()),
                                }
                            });
                        match result {
                            Ok(text) => {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Loot profile", &[extension])
                                    .set_file_name(format!("treasure.{}", extension))
                                    .save_file()
                                {
                                    self.start(&ctx, path, Some(text), extension == "toml");
                                }
                            }
                            Err(e) => self.notice = e,
                        }
                    }
                }
            });
            ui.add_space(12.0);
            if let Some(table) = self.value.as_table_mut() {
                theme::card().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    theme::eyebrow(ui, "PROFILE IDENTITY");
                    ui.add_space(8.0);
                    ui.columns(4, |columns| {
                        for (ui, (key, label)) in columns.iter_mut().zip([
                            ("treasure_type", "Treasure type"),
                            ("tier", "Tier"),
                            ("loot_quality_mod", "Loot quality modifier"),
                            ("unknown_chances", "Unknown chances"),
                        ]) {
                            if let Some(value) = table.get_mut(key) {
                                ui.label(label);
                                ui.push_id(key, |ui| {
                                    self.forms.field(ui, "", value);
                                });
                            }
                        }
                    });
                });
                ui.add_space(18.0);
                ui.columns(3, |columns| {
                    for (ui, (prefix, title)) in columns.iter_mut().zip([
                        ("item_", "Items"),
                        ("magic_item_", "Magic items"),
                        ("mundane_item_", "Mundane items"),
                    ]) {
                        theme::card().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.heading(title);
                            ui.add_space(8.0);
                            let selection = if prefix == "mundane_item_" {
                                "type_selection_chances"
                            } else {
                                "treasure_type_selection_chances"
                            };
                            for (suffix, label) in [
                                ("chance", "Chance (%)"),
                                ("min_amount", "Minimum amount"),
                                ("max_amount", "Maximum amount"),
                                (selection, "Treasure selection table"),
                            ] {
                                let key = format!("{prefix}{suffix}");
                                if let Some(value) = table.get_mut(&key) {
                                    ui.label(label);
                                    ui.push_id(&key, |ui| {
                                        self.forms.field(ui, "", value);
                                    });
                                    ui.add_space(8.0);
                                }
                            }
                        });
                    }
                });
            }
        });
        if self.busy() {
            ui.spinner();
        }
        theme::notice(ui, &self.notice);
        ui.small("Offline authoring only. Loot profiles are not yet accepted by the weenie pack builder or live publication.");
    }
}
impl Drop for LootEditor {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            let _ = job.thread.join();
        }
    }
}
