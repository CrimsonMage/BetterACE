use eframe::egui;
#[derive(Default)]
pub(crate) struct ScriptEditor {
    pub text: String,
    pub dirty: bool,
    pub notice: String,
}
pub(crate) enum ScriptAction {
    Open(std::path::PathBuf),
    Save(std::path::PathBuf, String),
    Apply(Vec<bace_content::Emote>),
}
impl ScriptEditor {
    pub fn ui(&mut self, ui: &mut egui::Ui, document: &toml::Value) -> Option<ScriptAction> {
        let mut action = None;
        crate::theme::subtitle(
            ui,
            "Edit event scripts, compile into this weenie, or export an .es file. TOML remains the saved document.",
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.dirty, egui::Button::new("Generate from weenie"))
                .on_hover_text("Apply or discard the script draft first.")
                .clicked()
            {
                match document
                    .clone()
                    .try_into::<bace_content::WeenieV1>()
                    .map_err(|e| e.to_string())
                    .and_then(|w| bace_import::export_emote_script(&w.properties.emotes))
                {
                    Ok(text) => {
                        self.text = text;
                        self.notice =
                            "Generated script. Relational IDs stay in the native document.".into();
                    }
                    Err(e) => self.notice = e,
                }
            }
            if ui
                .add_enabled(!self.dirty, egui::Button::new("Open .es…"))
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("EmoteScript", &["es"])
                    .pick_file()
            {
                action = Some(ScriptAction::Open(path));
            }
            if ui.button("Export .es…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("EmoteScript", &["es"])
                    .set_file_name("emotes.es")
                    .save_file()
            {
                action = Some(ScriptAction::Save(path, self.text.clone()));
            }
            if ui.add(crate::theme::primary("Compile & apply")).clicked() {
                match bace_import::import_emote_script(&self.text) {
                    Ok(emotes) => action = Some(ScriptAction::Apply(emotes)),
                    Err(e) => self.notice = e,
                }
            }
            if self.dirty && ui.button("Discard draft").clicked() {
                self.text.clear();
                self.dirty = false;
            }
        });
        ui.small("Compile & apply replaces the emote list, resets relational row IDs and creates an undo checkpoint. Export uses a new filename.");
        egui::CollapsingHeader::new("Syntax reference").show(ui,|ui| {
            ui.monospace("Use:\n    - Tell: Hello, traveler!\n    - InqQuest: example_quest\n        QuestSuccess:\n            - Tell: Welcome back.\n        QuestFailure:\n            - StampQuest: example_quest");
            ui.label("Use explicit named fields for advanced values, e.g. Delay: 1.5, AwardXP: Amount64: 9007199254740993. Unknown syntax is rejected with a line number.");
        });
        if ui
            .add(
                egui::TextEdit::multiline(&mut self.text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(22)
                    .char_limit(1024 * 1024),
            )
            .changed()
        {
            self.dirty = true;
        }
        crate::theme::notice(ui, &self.notice);
        action
    }
}
