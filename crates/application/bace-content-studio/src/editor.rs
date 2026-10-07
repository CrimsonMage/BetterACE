use crate::document::Document;
use crate::editor_io::{Job, Reply, Request};
use crate::form_schema::Section;
use crate::forms::{self, Forms};
use eframe::egui::{self, Color32};
use std::collections::BTreeMap;
use std::path::PathBuf;

enum Action {
    New,
    Open(PathBuf),
}

pub(crate) struct Editor {
    document: Option<Document>,
    sections: Vec<Section>,
    labels: BTreeMap<String, Vec<(i64, String)>>,
    forms: Forms,
    selected: usize,
    filter: String,
    page: usize,
    raw_mode: bool,
    preview_mode: bool,
    preview: crate::preview::Preview,
    raw: String,
    raw_dirty: bool,
    notice: String,
    job: Option<Job>,
    pending: Option<Action>,
    close_prompt: bool,
    discard_close: bool,
}

impl Default for Editor {
    fn default() -> Self {
        let (sections, notice) = match crate::form_schema::sections() {
            Ok(s) => (s, String::new()),
            Err(e) => (Vec::new(), e),
        };
        Self {
            document: None,
            sections,
            labels: forms::labels(),
            forms: Forms::default(),
            selected: 0,
            filter: String::new(),
            page: 0,
            raw_mode: false,
            preview_mode: false,
            preview: Default::default(),
            raw: String::new(),
            raw_dirty: false,
            notice,
            job: None,
            pending: None,
            close_prompt: false,
            discard_close: false,
        }
    }
}

impl Editor {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn dirty(&self) -> bool {
        self.raw_dirty || !self.forms.valid() || self.document.as_ref().is_some_and(Document::dirty)
    }
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.action(Action::Open(path), ctx);
    }

    fn action(&mut self, action: Action, ctx: &egui::Context) {
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }
    fn perform(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::New => match Document::new() {
                Ok(doc) => self.set_document(doc),
                Err(e) => self.notice = e,
            },
            Action::Open(path) => self.start_job(Request::Open(path), ctx),
        }
    }
    fn set_document(&mut self, doc: Document) {
        self.raw = doc.text().unwrap_or_default();
        self.document = Some(doc);
        self.forms.clear();
        self.raw_dirty = false;
        self.raw_mode = false;
        self.notice.clear();
        self.page = 0;
    }
    fn start_job(&mut self, request: Request, ctx: &egui::Context) {
        match Job::start(request, ctx.clone()) {
            Ok(job) => self.job = Some(job),
            Err(e) => self.notice = e,
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.job.as_mut().and_then(Job::poll) {
            self.job = None;
            match result {
                Ok(Reply::Exported { path, notes }) => {
                    self.notice = format!(
                        "Exported to {}\n{notes}\nThe complete native document is preserved in native.toml.",
                        path.display()
                    )
                }
                Ok(Reply::Opened(doc)) => self.set_document(doc),
                Ok(Reply::Saved { path, text }) => {
                    if let Some(doc) = &mut self.document {
                        doc.saved(path, text);
                    }
                    self.notice = "Saved native TOML.".into();
                    self.raw_dirty = false;
                    self.forms.clear();
                }
                Err(error) => self.notice = error,
            }
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    pub fn guard_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.discard_close
            && (self.dirty() || self.busy())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt = true;
        }
        if self.close_prompt {
            egui::Window::new("Unsaved editor changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(if self.busy() {
                        "Wait for the file operation to finish."
                    } else {
                        "Keep editing to save your changes, or discard them and close."
                    });
                    if ui.button("Keep editing").clicked() {
                        self.close_prompt = false;
                    }
                    if ui
                        .add_enabled(!self.busy(), egui::Button::new("Discard and close"))
                        .clicked()
                    {
                        self.discard_close = true;
                        self.close_prompt = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
        }
    }
    fn save(&mut self, save_as: bool, ctx: &egui::Context) {
        if self.raw_dirty || !self.forms.valid() {
            self.notice =
                "Apply the TOML source or correct invalid numeric fields before saving.".into();
            return;
        }
        let Some(doc) = &self.document else {
            return;
        };
        let text = match doc.validated() {
            Ok(t) => t,
            Err(e) => {
                self.notice = e;
                return;
            }
        };
        let path = if !save_as && doc.path.is_some() {
            doc.path.clone()
        } else {
            rfd::FileDialog::new()
                .add_filter("Native TOML", &["toml"])
                .set_file_name(format!(
                    "{}.toml",
                    doc.value
                        .get("weenie_id")
                        .and_then(toml::Value::as_integer)
                        .unwrap_or(1)
                ))
                .save_file()
        };
        if let Some(mut path) = path {
            if path.extension().is_none() {
                path.set_extension("toml");
            }
            if !path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("toml"))
            {
                self.notice = "Native documents must use a .toml filename.".into();
                return;
            }
            let expected = if doc.path.as_ref() == Some(&path) {
                doc.disk_hash
            } else {
                None
            };
            self.start_job(
                Request::Save {
                    path,
                    text,
                    expected,
                },
                ctx,
            );
        }
    }

    fn export(&mut self, format: crate::legacy_bundle::Format, ctx: &egui::Context) {
        if self.raw_dirty || !self.forms.valid() {
            self.notice = "Apply source changes or correct numeric errors before export.".into();
            return;
        }
        let Some(doc) = &self.document else {
            return;
        };
        let text = match doc.validated() {
            Ok(t) => t,
            Err(e) => {
                self.notice = e;
                return;
            }
        };
        if let Some(parent) = rfd::FileDialog::new()
            .set_title("Export folder (includes native TOML companion)")
            .pick_folder()
        {
            self.start_job(
                Request::Export {
                    text,
                    parent,
                    format,
                },
                ctx,
            );
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        if self.pending.is_some() {
            egui::Window::new("Replace unsaved document?")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label("Your current document has unsaved edits.");
                    if ui.button("Keep editing").clicked() {
                        self.pending = None;
                    }
                    if ui.button("Discard edits and continue").clicked()
                        && let Some(action) = self.pending.take()
                    {
                        self.perform(action, &ctx);
                    }
                });
        }
        ui.add_enabled_ui(!self.busy(), |ui| {
            if self.document.is_none() {
                self.welcome(ui);
                return;
            }
            ui.horizontal_wrapped(|ui| {
                if ui.add(crate::theme::primary("Save TOML")).clicked() {self.save(false, &ctx);}
                ui.menu_button("File", |ui| {
                    if ui.button("New weenie").clicked() {self.action(Action::New, &ctx);ui.close();}
                    if ui.button("Open TOML / JSON…").clicked() {self.pick_document(&ctx);ui.close();}
                    if ui.button("Save as…").clicked() {self.save(true,&ctx);ui.close();}
                    if ui.button("Clone as new").clicked() && let Some(doc)=&mut self.document {doc.clone_as_new();self.notice="Cloned. Set a new weenie ID and class name before saving.".into();ui.close();}
                });
                ui.menu_button("Export", |ui| {
                    if ui.button("Legacy JSON…").clicked() {self.export(crate::legacy_bundle::Format::Json,&ctx);ui.close();}
                    if ui.button("Legacy SQL…").clicked() {self.export(crate::legacy_bundle::Format::Sql,&ctx);ui.close();}
                });
                if ui.button("Undo").clicked() && let Some(doc)=&mut self.document {doc.undo();self.forms.clear();self.raw_dirty=false;self.raw=doc.text().unwrap_or_default();}
                if ui.button("Redo").clicked() && let Some(doc)=&mut self.document {doc.redo();self.forms.clear();self.raw_dirty=false;self.raw=doc.text().unwrap_or_default();}
                if ui.button("Validate").clicked() && let Some(doc)=&self.document {
                    self.notice=if !self.forms.valid() || self.raw_dirty {"Apply source changes / fix numeric errors first.".into()} else {doc.validated().map(|_|"Content structure is valid. Runtime references still require publication validation.".into()).unwrap_or_else(|e|e)};
                }
                ui.label(egui::RichText::new(if self.dirty() {"●  Unsaved"} else {"●  Saved"}).color(if self.dirty() {Color32::from_rgb(236,190,112)} else {crate::theme::MUTED}));
            });
            ui.add_space(5.0);
            if let Some(doc)=&self.document {
                crate::theme::subtitle(ui, &doc.path.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"Untitled · native TOML document".into()));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.selectable_label(!self.raw_mode&&!self.preview_mode,"Properties").clicked() && !self.raw_dirty {self.raw_mode=false;self.preview_mode=false;}
                if ui.selectable_label(self.raw_mode,"TOML source").clicked() {
                    if !self.raw_mode && let Some(doc)=&self.document {self.raw=doc.text().unwrap_or_default();}
                    self.raw_mode=true;self.preview_mode=false;
                }
            });
            if ui.selectable_label(self.preview_mode,"3D model & DIDs").clicked() {self.preview_mode=true;}
            crate::theme::notice(ui, &self.notice);
            ui.separator();
            if self.preview_mode {
                egui::ScrollArea::vertical().id_salt("preview-scroll").show(ui,|ui|self.preview.ui(ui,self.document.as_ref().map(|d|&d.value)));
                if let Some((setup,clothing,palette))=self.preview.take_apply() {
                    if !self.forms.valid() || self.raw_dirty {self.notice="Apply source changes and correct numeric errors before changing DIDs.".into();}
                    else if let Some(doc)=&mut self.document {
                        if let Some(rows)=doc.value.get_mut("properties").and_then(|p|p.get_mut("data_ids")).and_then(toml::Value::as_array_mut) {
                            for (id,value) in [(1,setup),(7,clothing),(6,palette)] {
                                if let Some(row)=rows.iter_mut().find(|p|p.get("id").and_then(toml::Value::as_integer)==Some(id)) {row["value"]=toml::Value::Integer(i64::from(value));}
                                else {let mut row=toml::Table::new();row.insert("id".into(),toml::Value::Integer(id));row.insert("value".into(),toml::Value::Integer(i64::from(value)));rows.push(toml::Value::Table(row));}
                            }
                            self.forms.clear();self.notice=doc.checkpoint().err().unwrap_or_else(||"Appearance DIDs updated. Save to keep these changes.".into());
                        }
                    }
                }
            } else if self.raw_mode {
                egui::ScrollArea::vertical().id_salt("source-scroll").show(ui, |ui| self.source_ui(ui));
            } else {
                egui::Panel::left("property-categories").exact_size(185.0).resizable(false)
                    .frame(egui::Frame::new().inner_margin(egui::Margin {right:14,..Default::default()}))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("categories").show(ui, |ui| self.categories(ui));
                    });
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt(("property-scroll",self.selected)).show(ui, |ui| self.properties_ui(ui));
                });
            }
        });
        if self.busy() {
            ui.spinner();
        }
    }

    fn pick_document(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Weenies", &["toml", "json"])
            .pick_file()
        {
            self.open(path, ctx);
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(30.0);
        crate::theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add_space(20.0);
            crate::theme::eyebrow(ui, "CREATE · INSPECT · REFINE");
            ui.label(
                egui::RichText::new("Give your world its details.")
                    .size(32.0)
                    .strong(),
            );
            crate::theme::subtitle(
                ui,
                "Edit weenie properties, behavior and appearance in one workspace.",
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if ui.add(crate::theme::primary("Create weenie")).clicked() {
                    self.action(Action::New, &ctx);
                }
                if ui.button("Open TOML / JSON…").clicked() {
                    self.pick_document(&ctx);
                }
            });
            ui.add_space(20.0);
        });
        ui.add_space(24.0);
        ui.columns(3, |columns| {
            for (ui, (number, title, body)) in columns.iter_mut().zip([
                (
                    "01",
                    "Author in TOML",
                    "Named properties, exact values and a source view. Undo edits as you work.",
                ),
                (
                    "02",
                    "Bring existing content",
                    "Convert legacy JSON and SQL, then open the native files in the editor.",
                ),
                (
                    "03",
                    "Build runtime packs",
                    "Compile a content folder into immutable .bace files for mapped access.",
                ),
            ]) {
                crate::theme::eyebrow(ui, number);
                ui.strong(title);
                crate::theme::subtitle(ui, body);
            }
        });
        crate::theme::notice(ui, &self.notice);
    }

    fn categories(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &self.document else {
            return;
        };
        for (index, section) in self.sections.iter().enumerate() {
            if let Some(title) = match index {
                0 => Some("PROPERTIES"),
                7 => Some("CHARACTER"),
                12 => Some("BEHAVIOR & WORLD"),
                17 => Some("APPEARANCE"),
                _ => None,
            } {
                ui.add_space(10.0);
                crate::theme::eyebrow(ui, title);
            }
            let count = doc
                .value
                .get("properties")
                .and_then(|p| p.get(section.key))
                .and_then(toml::Value::as_array)
                .map_or(0, Vec::len);
            let text = format!("{}   {}", section.label, count);
            if ui
                .add_sized(
                    [165.0, 30.0],
                    egui::Button::selectable(self.selected == index, text),
                )
                .clicked()
            {
                if self.forms.valid() {
                    self.selected = index;
                    self.page = 0;
                    self.filter.clear();
                    self.forms.clear();
                } else {
                    self.notice = "Correct invalid numeric fields before changing sections.".into();
                }
            }
        }
    }

    fn source_ui(&mut self, ui: &mut egui::Ui) {
        if self.document.is_none() {
            return;
        }
        if ui
            .add(
                egui::TextEdit::multiline(&mut self.raw)
                    .code_editor()
                    .desired_rows(24)
                    .desired_width(f32::INFINITY)
                    .char_limit(16 * 1024 * 1024),
            )
            .changed()
        {
            self.raw_dirty = true;
        }
        if ui.button("Apply TOML changes").clicked() {
            match bace_content_tools::parse(&self.raw).and_then(|t| bace_content_tools::export(&t))
            {
                Ok(text) => match toml::from_str::<toml::Value>(&text) {
                    Ok(value) => {
                        if let Some(doc) = &mut self.document {
                            doc.value = value;
                            self.notice = doc.checkpoint().err().unwrap_or_default();
                            self.raw_dirty = false;
                            self.forms.clear();
                        }
                    }
                    Err(e) => self.notice = e.to_string(),
                },
                Err(e) => self.notice = e.to_string(),
            }
        }
    }

    fn properties_ui(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let mut changed = false;
        egui::CollapsingHeader::new("Document identity")
            .default_open(true)
            .show(ui, |ui| {
                crate::theme::card().show(ui, |ui| {
                    for key in ["weenie_id", "class_name", "weenie_type"] {
                        if let Some(value) = doc.value.get_mut(key) {
                            ui.push_id(key, |ui| {
                                changed |= self.forms.field(ui, key, value);
                            });
                        }
                    }
                });
            });
        ui.add_space(10.0);
        let Some(section) = self.sections.get(self.selected) else {
            return;
        };
        ui.heading(section.label);
        let Some(properties) = doc
            .value
            .get_mut("properties")
            .and_then(toml::Value::as_table_mut)
        else {
            return;
        };
        if section.key == "book_pages" {
            let mut has_book = properties.contains_key("book");
            if ui.checkbox(&mut has_book, "Book limits").changed() {
                if has_book {
                    if let Ok(book) = crate::form_schema::book() {
                        properties.insert("book".into(), book);
                    }
                } else {
                    properties.remove("book");
                }
                changed = true;
            }
            if let Some(book) = properties.get_mut("book") {
                ui.push_id("book", |ui| {
                    changed |= self.forms.field(ui, "book", book);
                });
            }
        }
        let Some(array) = properties
            .entry(section.key.to_string())
            .or_insert_with(|| toml::Value::Array(Vec::new()))
            .as_array_mut()
        else {
            return;
        };
        let labels = self.labels.get(section.key);
        if section.key == "book_pages"
            && ui
                .button("Renumber SQL page IDs to match this list")
                .clicked()
        {
            for (index, page) in array.iter_mut().enumerate() {
                if let Some(table) = page.as_table_mut() {
                    table.insert("legacy_page_id".into(), toml::Value::Integer(index as i64));
                }
            }
            changed = true;
            self.forms.clear();
        }
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Search by ID or name…")
                        .desired_width(250.0),
                )
                .changed()
            {
                self.page = 0;
            }
            if ui
                .add_enabled(array.len() < 100_000, egui::Button::new("Add entry"))
                .clicked()
            {
                let mut row = section.prototype.clone();
                if section.dictionary {
                    let next = (1_i64..=100_001)
                        .find(|id| {
                            !array
                                .iter()
                                .any(|v| v.get("id").and_then(toml::Value::as_integer) == Some(*id))
                        })
                        .unwrap_or(1);
                    row["id"] = toml::Value::Integer(next);
                }
                array.push(row);
                changed = true;
                self.page = array.len().saturating_sub(1) / 50;
            }
        });
        let filter = self.filter.to_lowercase();
        let indices: Vec<_> = array
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if filter.is_empty() {
                    return true;
                }
                let id = row.get("id").and_then(toml::Value::as_integer);
                id.is_some_and(|id| {
                    id.to_string().contains(&filter)
                        || labels.is_some_and(|l| {
                            l.iter()
                                .any(|(n, name)| *n == id && name.to_lowercase().contains(&filter))
                        })
                })
            })
            .map(|(index, _)| index)
            .collect();
        let pages = indices.len().div_ceil(50).max(1);
        self.page = self.page.min(pages - 1);
        if pages > 1 {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.page > 0, egui::Button::new("Previous"))
                    .clicked()
                {
                    self.page -= 1;
                }
                ui.label(format!("Page {} / {pages}", self.page + 1));
                if ui
                    .add_enabled(self.page + 1 < pages, egui::Button::new("Next"))
                    .clicked()
                {
                    self.page += 1;
                }
            });
        }
        if indices.is_empty() {
            crate::theme::card().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong(if array.is_empty() {
                    "No entries yet"
                } else {
                    "No matching entries"
                });
                crate::theme::subtitle(ui, "Add an entry or adjust your search to get started.");
            });
        }
        let mut remove = None;
        let mut duplicate = None;
        let mut swap = None;
        let can_duplicate = array.len() < 100_000;
        for &index in indices.iter().skip(self.page * 50).take(50) {
            let row = &mut array[index];
            let id = row.get("id").and_then(toml::Value::as_integer);
            let name = id
                .and_then(|id| labels.and_then(|labels| labels.iter().find(|(n, _)| *n == id)))
                .map(|(_, name)| name.as_str())
                .unwrap_or("Entry");
            crate::theme::card().inner_margin(12).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::CollapsingHeader::new(format!(
                    "{} · {}",
                    id.map(|n| n.to_string())
                        .unwrap_or_else(|| (index + 1).to_string()),
                    name
                ))
                .default_open(indices.len() <= 4)
                .id_salt((section.key, index))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Remove").clicked() {
                            remove = Some(index);
                        }
                        if ui
                            .add_enabled(can_duplicate, egui::Button::new("Duplicate"))
                            .clicked()
                        {
                            duplicate = Some(index);
                        }
                        if !section.dictionary && index > 0 && ui.button("Move up").clicked() {
                            swap = Some((index, index - 1));
                        }
                    });
                    if section.dictionary
                        && let Some(labels) = labels
                    {
                        ui.menu_button("Choose named property…", |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for (id, name) in labels.iter().filter(|(id, name)| {
                                        filter.is_empty()
                                            || id.to_string().contains(&filter)
                                            || name.to_lowercase().contains(&filter)
                                    }) {
                                        if ui.button(format!("{id} · {name}")).clicked() {
                                            row["id"] = toml::Value::Integer(*id);
                                            changed = true;
                                            self.forms.clear();
                                            ui.close();
                                        }
                                    }
                                });
                        });
                    }
                    changed |= self.forms.field(ui, section.key, row);
                    changed |= self.forms.optionals(ui, section.key, row);
                });
            });
        }
        if let Some(index) = remove {
            array.remove(index);
            self.forms.clear();
            changed = true;
        } else if let Some(index) = duplicate
            && array.len() < 100_000
        {
            array.insert(index + 1, array[index].clone());
            self.forms.clear();
            changed = true;
        } else if let Some((a, b)) = swap {
            array.swap(a, b);
            self.forms.clear();
            changed = true;
        }
        if changed {
            self.notice = doc.checkpoint().err().unwrap_or_default();
        }
    }
}
