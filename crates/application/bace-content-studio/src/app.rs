use crate::ConversionOptions;
use crate::worker::{FileResult, MAX_FILES, Worker};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Studio {
    editor: crate::editor::Editor,
    import_tab: bool,
    pack_tab: bool,
    pack_builder: crate::pack_builder::PackBuilder,
    files: Vec<PathBuf>,
    output: Option<PathBuf>,
    mariadb: Option<PathBuf>,
    worker: Option<Worker>,
    results: Vec<FileResult>,
    notice: String,
    cancelling: bool,
    close_when_done: bool,
}

impl Studio {
    fn add_files(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        for path in files {
            let supported = path
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("json") || x.eq_ignore_ascii_case("sql"));
            if !supported {
                self.notice = "Only .json and .sql files can be added.".into();
            } else if !self.files.contains(&path) {
                if self.files.len() == MAX_FILES {
                    self.notice = format!("A batch can contain at most {MAX_FILES} files.");
                    break;
                }
                self.files.push(path);
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        while let Some(result) = worker.receive() {
            self.results.push(result);
        }
        if worker.finished() {
            // Drain again after observing completion: the final send may have
            // happened between the first drain and is_finished().
            while let Some(result) = worker.receive() {
                self.results.push(result);
            }
            if let Some(mut worker) = self.worker.take() {
                let outcome = worker.finish();
                let succeeded = self.results.iter().filter(|r| r.result.is_ok()).count();
                self.notice = match outcome {
                    Err(error) => error,
                    Ok(()) => format!(
                        "{}: {succeeded} succeeded, {} failed, {} not processed.",
                        if self.cancelling {
                            "Stopped"
                        } else {
                            "Finished"
                        },
                        self.results.len() - succeeded,
                        self.files.len() - self.results.len()
                    ),
                };
            }
            if self.close_when_done {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(output_parent) = self.output.clone() else {
            return;
        };
        match Worker::start(
            self.files.clone(),
            ConversionOptions {
                output_parent,
                mariadb_basedir: self.mariadb.clone(),
            },
            ctx.clone(),
        ) {
            Ok(worker) => {
                self.worker = Some(worker);
                self.results.clear();
                self.notice.clear();
                self.cancelling = false;
            }
            Err(error) => self.notice = error,
        }
    }

    fn inputs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("01   Source files");
            ui.label(format!("{} / {MAX_FILES}", self.files.len()));
            if ui.button("Add files…").clicked()
                && let Some(files) = rfd::FileDialog::new()
                    .add_filter("Weenie content", &["json", "sql"])
                    .pick_files()
            {
                self.add_files(files);
            }
            if ui.button("Clear").clicked() {
                self.files.clear();
            }
        });
        ui.label("Choose JSON or SQL files, or drag them into this window.");
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("sources")
            .max_height(170.0)
            .show(ui, |ui| {
                if self.files.is_empty() {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("Drop JSON or SQL files here").size(20.0));
                        crate::theme::subtitle(ui, "or use Add files to browse your computer");
                        ui.add_space(20.0);
                    });
                    ui.add_space(16.0);
                }
                for (index, path) in self.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                        ui.label(path.file_name().unwrap_or_default().to_string_lossy())
                            .on_hover_text(path.display().to_string());
                    });
                }
            });
        if let Some(index) = remove {
            self.files.remove(index);
        }
        ui.separator();
        ui.strong("02   Destination");
        folder_picker(ui, &mut self.output, "Choose output folder…");
        ui.label(
            "Each source gets a new subfolder with native TOML files and a conversion manifest.",
        );
        let has_sql = self
            .files
            .iter()
            .any(|p| p.extension().is_some_and(|s| s.eq_ignore_ascii_case("sql")));
        if has_sql {
            ui.add_space(10.0);
            ui.heading("SQL setup");
            if cfg!(target_os = "linux") {
                folder_picker(
                    ui,
                    &mut self.mariadb,
                    "Choose private MariaDB installation…",
                );
                ui.label("Select the folder containing bin/mariadb, bin/mariadbd and bin/mariadb-install-db.");
                ui.label("SQL runs in a disposable local database. Full-world dumps with non-weenie tables are unsupported.");
            } else {
                ui.colored_label(
                    Color32::YELLOW,
                    "SQL conversion currently requires Linux. JSON conversion is available here.",
                );
            }
        }
    }

    fn results(&mut self, ui: &mut egui::Ui) {
        let mut edit_path = None;
        if self.results.is_empty() {
            return;
        }
        ui.add_space(12.0);
        ui.heading("Conversion results");
        for item in &self.results {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(
                    RichText::new(
                        item.source
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    )
                    .strong(),
                );
                match &item.result {
                    Ok(summary) => {
                        ui.colored_label(
                            Color32::LIGHT_GREEN,
                            format!("Converted {} weenies", summary.weenies),
                        );
                        ui.label(summary.directory.display().to_string());
                        if ui.small_button("Copy output path").clicked() {
                            ui.ctx().copy_text(summary.directory.display().to_string());
                        }
                        if ui
                            .add_enabled(
                                !self.editor.busy(),
                                egui::Button::new("Edit converted weenie…"),
                            )
                            .clicked()
                        {
                            edit_path = rfd::FileDialog::new()
                                .add_filter("Native TOML", &["toml"])
                                .set_directory(&summary.directory)
                                .pick_file();
                        }
                    }
                    Err(error) => {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    }
                }
            });
        }
        if let Some(path) = edit_path {
            self.editor.open(path, ui.ctx());
            self.import_tab = false;
            self.pack_tab = false;
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.editor.poll(&ctx);
        self.editor.guard_close(&ctx);
        self.pack_builder.poll(&ctx);
        self.poll(&ctx);
        if ctx.input(|i| i.viewport().close_requested())
            && let Some(worker) = &self.worker
        {
            worker.cancel();
            self.cancelling = true;
            self.close_when_done = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.worker.is_none() && self.import_tab {
            let dropped = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>()
            });
            self.add_files(dropped);
        }
        egui::Panel::left("studio-navigation")
            .exact_size(184.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::PANEL)
                    .inner_margin(18),
            )
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("B / ACE")
                        .size(25.0)
                        .strong()
                        .color(crate::theme::ACCENT),
                );
                ui.label(
                    RichText::new("CONTENT STUDIO")
                        .size(10.0)
                        .color(crate::theme::MUTED),
                );
                ui.add_space(35.0);
                crate::theme::eyebrow(ui, "WORKSPACE");
                for (index, label) in ["Weenie editor", "Legacy import", "Pack builder"]
                    .iter()
                    .enumerate()
                {
                    let selected = if self.pack_tab {
                        2
                    } else {
                        usize::from(self.import_tab)
                    };
                    if ui
                        .add_sized(
                            [148.0, 42.0],
                            egui::Button::selectable(selected == index, *label),
                        )
                        .clicked()
                    {
                        self.import_tab = index == 1;
                        self.pack_tab = index == 2;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.small("BetterACEmulator");
                    ui.label(
                        RichText::new("LOCAL AUTHORING")
                            .size(10.0)
                            .color(crate::theme::ACCENT),
                    );
                    ui.separator();
                    crate::theme::subtitle(ui, "TOML source\nBinary runtime packs");
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::BG).inner_margin(24))
            .show(ui, |ui| {
                crate::theme::eyebrow(ui, "BETTERACE  /  CONTENT");
                ui.heading(if self.pack_tab {"Pack builder"} else if self.import_tab {"Legacy import"} else {"Weenie editor"});
                ui.add_space(12.0);
                if !self.pack_tab && !self.import_tab {
                    self.editor.ui(ui);
                    return;
                }
            egui::ScrollArea::vertical().id_salt("workflow").show(ui, |ui| {
                if self.pack_tab {self.pack_builder.ui(ui);return;}
                ui.label("Convert legacy weenie content into editable native TOML.");
                ui.add_space(16.0);
                ui.add_enabled_ui(self.worker.is_none(), |ui| {crate::theme::card().show(ui, |ui| {ui.set_min_width(ui.available_width());self.inputs(ui);});});
                ui.add_space(12.0);
                if let Some(worker) = &self.worker {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{} {} of {} files", if self.cancelling { "Stopping after cleanup ·" } else { "Converting ·" }, self.results.len(), self.files.len()));
                        if ui.add_enabled(!self.cancelling, egui::Button::new("Cancel remaining")).clicked() {
                            worker.cancel();
                            self.cancelling = true;
                        }
                    });
                    ui.label("An active SQL operation may need time to finish and clean up.");
                } else {
                    let has_sql = self.files.iter().any(|p| p.extension().is_some_and(|s| s.eq_ignore_ascii_case("sql")));
                    let ready = !self.files.is_empty() && self.output.is_some()
                        && (!has_sql || (cfg!(target_os = "linux") && self.mariadb.is_some()));
                    if ui.add_enabled(ready, crate::theme::primary("Convert to TOML")).clicked() {
                        self.start(&ctx);
                    }
                }
                crate::theme::notice(ui, &self.notice);
                self.results(ui);
                ui.add_space(16.0);
                ui.separator();
                ui.small("Exports are local authoring files. Importing them into a live server is a separate step.");
            });
        });
    }
}

fn folder_picker(ui: &mut egui::Ui, value: &mut Option<PathBuf>, label: &str) {
    ui.horizontal_wrapped(|ui| {
        if ui.button(label).clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            *value = Some(path);
        }
        ui.label(
            value
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Not selected".into()),
        );
    });
}
