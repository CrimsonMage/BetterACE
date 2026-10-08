use crate::{
    clothing_document::{ClothingDocument, Job, Reply, Request},
    clothing_forms::ClothingForms,
    preview::Preview,
    theme,
};
use bace_content::ClothingPatchV1;
use eframe::egui;
#[derive(Default)]
pub(crate) struct ClothingEditor {
    document: Option<ClothingDocument>,
    forms: ClothingForms,
    preview: Preview,
    job: Option<Job>,
    notice: String,
    mode: usize,
    source: String,
    source_dirty: bool,
    undo: Option<ClothingPatchV1>,
    close_prompt: bool,
}
impl ClothingEditor {
    pub fn patch(&self) -> Option<&ClothingPatchV1> {
        // Invalid drafts must reach preview validation, never silently show base DAT.
        self.document.as_ref().map(|d| &d.patch)
    }
    pub fn source_pending(&self) -> bool {
        self.source_dirty
    }
    pub fn dirty(&self) -> bool {
        self.source_dirty || self.document.as_ref().is_some_and(ClothingDocument::dirty)
    }
    fn set_document(&mut self, doc: ClothingDocument) {
        self.preview.use_clothing(&doc.patch);
        self.source.clear();
        self.source_dirty = false;
        self.undo = None;
        self.forms = Default::default();
        self.document = Some(doc);
        self.notice.clear();
    }
    fn start(&mut self, request: Request, ctx: &egui::Context) {
        match Job::start(request, ctx.clone()) {
            Ok(job) => self.job = Some(job),
            Err(e) => self.notice = e,
        }
    }
    fn copy_selected(&mut self, setup: bool) {
        let part = setup
            .then(|| self.preview.selected_setup().cloned())
            .flatten();
        let palette = (!setup)
            .then(|| self.preview.selected_template().cloned())
            .flatten();
        if let Some(doc) = &mut self.document {
            match doc.copy_entries(part, palette) {
                Ok(previous) => {
                    self.undo = Some(previous);
                    self.forms = Default::default();
                    self.notice = "Copied the complete selected entry into this ClothingBase. Other entries were preserved; Undo restores the previous document.".into();
                }
                Err(error) => self.notice = error,
            }
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.job.as_mut().and_then(Job::poll) {
            self.job = None;
            match result {
                Ok(Reply::Opened(doc)) => self.set_document(doc),
                Ok(Reply::Written { path, hash, native }) => {
                    if native && let Some(doc) = &mut self.document {
                        doc.saved = Some(doc.patch.clone());
                        doc.path = Some(path.clone());
                        doc.hash = Some(hash);
                    }
                    self.notice = format!("Wrote {}", path.display());
                }
                Err(e) => self.notice = e,
            }
        }
        if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    pub fn guard_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && (self.dirty() || self.job.is_some()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt = true;
        }
        if self.close_prompt {
            egui::Window::new("Unsaved ClothingBase changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Save the ClothingBase TOML or discard its changes before closing.");
                    if ui.button("Keep editing").clicked() {
                        self.close_prompt = false;
                    }
                    if ui
                        .add_enabled(self.job.is_none(), egui::Button::new("Discard and close"))
                        .clicked()
                    {
                        self.document = None;
                        self.source_dirty = false;
                        self.close_prompt = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
        }
    }
    fn save(&mut self, ctx: &egui::Context, as_new: bool, json: bool) {
        if self.source_dirty {
            self.notice = "Apply or discard the TOML draft before saving.".into();
            return;
        }
        let Some(doc) = &self.document else {
            return;
        };
        let text = if json {
            bace_import::export_clothing_json(&doc.patch)
        } else {
            doc.text()
        };
        let text = match text {
            Ok(t) => t,
            Err(e) => {
                self.notice = e;
                return;
            }
        };
        let existing = !as_new && !json && doc.path.is_some();
        let path = if existing {
            doc.path.clone()
        } else {
            let ext = if json { "json" } else { "toml" };
            rfd::FileDialog::new()
                .add_filter("ClothingBase", &[ext])
                .set_file_name(format!("{:08X}.{ext}", doc.patch.id))
                .save_file()
        };
        if let Some(path) = path {
            let expected = if existing { doc.hash } else { None };
            self.start(
                Request::Write {
                    path,
                    text,
                    expected,
                    native: !json,
                },
                ctx,
            );
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        theme::subtitle(
            ui,
            "Author all body-part, texture and palette changes for one ClothingBase. Native TOML stays separate from your weenie.",
        );
        ui.add_enabled_ui(self.job.is_none(),|ui|{
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.add_enabled(!self.dirty(), egui::Button::new("New ClothingBase")).clicked() {
                        self.set_document(ClothingDocument::new(ClothingPatchV1 {
                            schema_version: 1, id: 0x10ffffff, setups: Vec::new(), palettes: Vec::new(),
                        }));
                        self.mode = 0;
                        ui.close();
                    }
                    if ui.add_enabled(self.document.is_some(), egui::Button::new("Save as…")).clicked() {
                        self.save(&ctx, true, false);
                        ui.close();
                    }
                    if ui.add_enabled(self.document.is_some(), egui::Button::new("Export mod JSON…")).clicked() {
                        self.save(&ctx, true, true);
                        ui.close();
                    }
                    ui.separator();
                    if ui.add_enabled(self.document.is_some() && !self.dirty(), egui::Button::new("Close document")).clicked() {
                        self.document = None;
                        self.undo = None;
                        self.source.clear();
                        ui.close();
                    }
                    if ui.add_enabled(self.dirty(), egui::Button::new("Discard changes")).clicked() {
                        if let Some(saved) = self.document.as_ref().and_then(|d| d.saved.clone()) {
                            if let Some(doc) = &mut self.document { doc.patch = saved; }
                        } else { self.document = None; }
                        self.source_dirty = false;
                        self.source.clear();
                        self.undo = None;
                        self.forms = Default::default();
                        ui.close();
                    }
                });
                if ui.add_enabled(!self.dirty(), egui::Button::new("Open TOML / mod JSON…"))
                    .on_hover_text("Save or discard the current ClothingBase before opening another.").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("ClothingBase", &["toml", "json"]).pick_file() {
                    self.start(Request::Open(path), &ctx);
                }
                if ui.add_enabled(self.document.is_some(), theme::primary("Save TOML")).clicked() {
                    self.save(&ctx, false, false);
                }
                if ui.add_enabled(self.undo.is_some() && !self.source_dirty, egui::Button::new("Undo / redo")).clicked()
                    && let (Some(doc), Some(previous)) = (&mut self.document, self.undo.take()) {
                    self.undo = Some(std::mem::replace(&mut doc.patch, previous));
                    self.forms = Default::default();
                }
            });
            if let Some(doc)=&self.document{ui.small(format!("{} · {}",doc.path.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"Unsaved ClothingBase".into()),if self.dirty(){"Modified"}else{"Saved"}));}
            ui.horizontal(|ui|{
                ui.selectable_value(&mut self.mode,0,"Structured changes");
                if ui.selectable_label(self.mode==1,"TOML source").clicked(){if !self.source_dirty{self.source=self.document.as_ref().map(|d|toml::to_string_pretty(&d.patch).unwrap_or_default()).unwrap_or_default();}self.mode=1;}
                if ui.selectable_label(self.mode==2,"Preview & DAT lookup").clicked(){if let Some(doc)=&self.document{self.preview.use_clothing(&doc.patch);}self.mode=2;}
            });
            theme::notice(ui,&self.notice);
            ui.separator();
            match self.mode {
                1=>{
                    ui.horizontal(|ui|{
                        if ui.add_enabled(self.document.is_some(),egui::Button::new("Apply TOML draft")).clicked(){match crate::clothing_document::native_parse(&self.source){Ok(patch)=>{if let Some(doc)=&mut self.document{self.undo=Some(std::mem::replace(&mut doc.patch,patch));}self.forms=Default::default();self.source_dirty=false;self.notice="Applied native ClothingBase changes.".into();},Err(e)=>self.notice=e}}
                        if ui.button("Discard draft").clicked(){self.source_dirty=false;self.source=self.document.as_ref().map(|d|toml::to_string_pretty(&d.patch).unwrap_or_default()).unwrap_or_default();}
                    });
                    if ui.add(egui::TextEdit::multiline(&mut self.source).code_editor().desired_width(f32::INFINITY).desired_rows(24).char_limit(bace_import::MAX_CLOTHING_BYTES)).changed(){self.source_dirty=true;}
                },
                2=>{
                    if self.source_dirty{ui.label("Apply or discard your TOML draft to preview its changes.");return;}
                    ui.horizontal_wrapped(|ui|{
                        if ui.add_enabled(!self.dirty()&&self.preview.clothing().is_some(),egui::Button::new("Copy inspected table into document")).clicked()&&let Some(patch)=self.preview.clothing().cloned(){self.set_document(ClothingDocument::new(patch));}
                        ui.small("Load a DAT table or preview the current override. Preview controls do not edit the document.");
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(self.document.is_some() && self.preview.selected_setup().is_some(), egui::Button::new("Copy selected setup into draft")).on_hover_text("Replaces the matching setup's whole part list, or adds a new variant. Keeps this document's ClothingBase DID.").clicked() {
                            self.copy_selected(true);
                        }
                        if ui.add_enabled(self.document.is_some() && self.preview.selected_template().is_some(), egui::Button::new("Copy selected template into draft")).on_hover_text("Copies every palette source and range for this template, with undo.").clicked() {
                            self.copy_selected(false);
                        }
                    });
                    let patch=self.document.as_ref().map(|d|&d.patch);
                    self.preview.ui(ui,None,patch);
                },
                _=>{
                    if let Some(doc)=&mut self.document{
                        if self.source_dirty{ui.label("Apply or discard the TOML draft before changing structured fields.");return;}
                        let before=doc.patch.clone();
                        if self.forms.ui(ui,&mut doc.patch){
                            // One bounded undo snapshot; refuse UI growth beyond the file limit.
                            if toml::to_string(&doc.patch).is_ok_and(|s|s.len()<=bace_import::MAX_CLOTHING_BYTES){self.undo=Some(before);}else{doc.patch=before;self.notice="ClothingBase exceeds 1 MiB; edit was not applied.".into();}
                        }
                        if let Err(e)=doc.patch.validate(){theme::notice(ui,&format!("Needs attention before preview/save: {e}"));}
                    }else{theme::card().show(ui,|ui|{ui.heading("Multiple changes, one ClothingBase");ui.label("Open a CustomClothingBase JSON file, create native TOML, or use Preview & DAT lookup to copy a table from your client DAT.");ui.label("A setup variant contains every model and texture change. A palette template contains every palette source and color range.");});}
                }
            }
        });
        if self.job.is_some() {
            ui.spinner();
        }
        ui.add_space(12.0);
        ui.small("Active valid overrides are also available in the weenie's 3D view when its ClothingBase DID matches. Offline preview/export only; no DAT modification or live-server publication.");
    }
}
