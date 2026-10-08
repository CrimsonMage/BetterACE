use crate::offline_workspace::{self, Review};
use eframe::egui;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use crate::preview_render::{self, Camera};
use crate::preview_scene::{self, PreviewRequest, PreviewScene};
use bace_storage_codec::{PackGeneration, PackKey, PackLimits, PackLookup, load_manifest};

fn content_type_label(namespace: u16) -> Option<&'static str> {
    match namespace {
        1 => Some("Weenies"),
        46 => Some("Loot profiles"),
        47 => Some("Rare profiles"),
        50 => Some("ClothingBase"),
        51 => Some("Animation part swaps"),
        _ => crate::offline_workspace::world_kind(namespace),
    }
}

#[derive(Default)]
pub(crate) struct OfflineWorkspaceUi {
    manifest_path: Option<PathBuf>,
    source: Option<PathBuf>,
    output: Option<PathBuf>,
    generation: Option<PackGeneration>,
    namespace: String,
    id: String,
    new_id: String,
    after: Option<PackKey>,
    page: Vec<PackKey>,
    record: Option<PackKey>,
    document: String,
    document_baseline: String,
    document_path: Option<PathBuf>,
    document_hash: Option<[u8; 32]>,
    field_form: bool,
    field_drafts: BTreeMap<String, String>,
    review: Option<Review>,
    review_job: Option<ReviewJob>,
    confirmed: bool,
    confirm_compaction: bool,
    job: Option<BuildJob>,
    portal: Option<PathBuf>,
    pose_setup: String,
    pose_animation: String,
    pose_object: String,
    pose_part: u16,
    pose_frame: usize,
    pose_job: Option<PoseJob>,
    pose_scene: Option<PreviewScene>,
    pose_texture: Option<egui::TextureHandle>,
    pose_camera: Camera,
    emote_weenie: String,
    emote_category: String,
    emote_trace: Option<crate::emote_sandbox::Trace>,
    notice: String,
    pending_record: Option<(PackKey, Option<PathBuf>)>,
    close_warning: bool,
    recipe: crate::recipe_workspace::RecipeWorkspace,
}

struct BuildJob {
    receiver: Receiver<Result<PathBuf, String>>,
    thread: JoinHandle<()>,
}

struct ReviewJob {
    receiver: Receiver<Result<Review, String>>,
    thread: JoinHandle<()>,
}

struct PoseJob {
    receiver: Receiver<Result<PreviewScene, String>>,
    thread: JoinHandle<()>,
}

struct LoadedDraft {
    text: String,
    path: Option<PathBuf>,
    hash: Option<[u8; 32]>,
}

impl OfflineWorkspaceUi {
    pub fn busy(&self) -> bool {
        self.job.is_some()
            || self.review_job.is_some()
            || self.pose_job.is_some()
            || self.recipe.busy()
    }

    pub fn dirty(&self) -> bool {
        self.record.is_some() && self.document != self.document_baseline
    }

    pub fn guard_close(&mut self, ctx: &egui::Context) {
        if self.dirty() && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_warning = true;
        }
        if self.close_warning {
            egui::Window::new("Unsaved offline draft")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Save the draft before closing, or discard your edits.");
                    if ui.button("Keep editing").clicked() {
                        self.close_warning = false;
                    }
                    if ui.button("Discard edits and close").clicked() {
                        self.document_baseline = self.document.clone();
                        self.close_warning = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.recipe.poll(ctx);
        if self
            .review_job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
        {
            let job = self.review_job.take().expect("finished review job");
            let panicked = job.thread.join().is_err();
            if panicked {
                self.notice = "Review worker stopped unexpectedly".into();
            } else {
                match job.receiver.try_recv() {
                    Ok(Ok(review)) => {
                        self.notice =
                            format!("{} changed records and indexes", review.changes.len());
                        self.review = Some(review);
                        self.confirmed = false;
                        self.confirm_compaction = false;
                    }
                    Ok(Err(error)) => self.notice = error,
                    Err(error) => self.notice = error.to_string(),
                }
            }
        } else if self.review_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self
            .pose_job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
        {
            let job = self.pose_job.take().expect("finished pose job");
            let panicked = job.thread.join().is_err();
            if panicked {
                self.notice = "DAT pose worker stopped unexpectedly".into();
            } else {
                match job.receiver.try_recv() {
                    Ok(Ok(scene)) => {
                        self.pose_scene = Some(scene);
                        self.pose_camera = Camera::default();
                        self.raster_pose(ctx);
                        self.notice = "Loaded selected DAT pose and native part swaps".into();
                    }
                    Ok(Err(error)) => self.notice = error,
                    Err(error) => self.notice = error.to_string(),
                }
            }
        } else if self.pose_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self
            .job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
        {
            let job = self.job.take().expect("finished job");
            let panicked = job.thread.join().is_err();
            self.notice = if panicked {
                "Candidate builder stopped unexpectedly".into()
            } else {
                match job.receiver.try_recv() {
                    Ok(Ok(path)) => format!(
                        "Candidate built in {}. The server has not accepted it.",
                        path.display()
                    ),
                    Ok(Err(error)) => error,
                    Err(error) => error.to_string(),
                }
            };
        } else if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn raster_pose(&mut self, ctx: &egui::Context) {
        if let Some(scene) = &self.pose_scene {
            match preview_render::render(scene, self.pose_camera) {
                Ok(image) => {
                    self.pose_texture =
                        Some(ctx.load_texture("offline-pose", image, egui::TextureOptions::LINEAR))
                }
                Err(error) => self.notice = error,
            }
        }
    }

    fn load_pose(&mut self) {
        let Some(path) = self.portal.clone() else {
            self.notice = "Choose client_portal.dat".into();
            return;
        };
        let parse = |text: &str| {
            u32::from_str_radix(
                text.trim()
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .map_err(|e| e.to_string())
        };
        let (setup, animation_id) = match (parse(&self.pose_setup), parse(&self.pose_animation)) {
            (Ok(setup), Ok(animation)) => (setup, animation),
            _ => {
                self.notice = "Enter hexadecimal Setup and Animation DIDs".into();
                return;
            }
        };
        let patch = if self.record.is_some_and(|key| key.namespace == 51) {
            match bace_content_tools::parse_animation_swap(&self.document) {
                Ok(patch) if patch.animation_id == animation_id => Some(patch),
                Ok(_) => {
                    self.notice =
                        "Draft animation DID does not match the selected animation".into();
                    return;
                }
                Err(error) => {
                    self.notice = error;
                    return;
                }
            }
        } else {
            None
        };
        let clothing_patch = if self.record.is_some_and(|key| key.namespace == 50) {
            match bace_content_tools::parse_clothing_patch(&self.document) {
                Ok(patch) => Some(patch),
                Err(error) => {
                    self.notice = error;
                    return;
                }
            }
        } else {
            None
        };
        let clothing = clothing_patch.as_ref().map_or(0, |patch| patch.id);
        let template = clothing_patch
            .as_ref()
            .and_then(|patch| patch.palettes.first())
            .map_or(0, |palette| palette.template);
        let request = PreviewRequest {
            path,
            setup,
            clothing,
            palette: 0,
            template,
            shade: 0.5,
            appearance: None,
            clothing_patch,
            animation_id,
            animation_frame: self.pose_frame,
            animation_patch: patch,
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("studio-animation-pose".into())
            .spawn(move || {
                let _ = sender.send(preview_scene::load(request));
            }) {
            Ok(thread) => {
                self.pose_job = Some(PoseJob { receiver, thread });
                self.notice = "Loading DAT pose…".into();
            }
            Err(error) => self.notice = error.to_string(),
        }
    }

    fn new_animation_patch(&mut self) {
        if self.dirty() {
            self.notice = "Save or discard the current draft first".into();
            return;
        }
        let Some(source) = &self.source else {
            self.notice = "Choose a native TOML folder".into();
            return;
        };
        let parse = |text: &str| {
            u32::from_str_radix(
                text.trim()
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .map_err(|e| e.to_string())
        };
        let result = (|| -> Result<(PackKey, String, PathBuf), String> {
            let animation_id = parse(&self.pose_animation)?;
            let object_id = parse(&self.pose_object)?;
            let patch = bace_content::AnimationSwapPatchV1 {
                schema_version: 1,
                animation_id,
                edits: vec![bace_content::AnimationSwapEditV1 {
                    operation: bace_content::AnimationSwapOperationV1::Insert,
                    frame: self.pose_frame as u32,
                    hook_index: 0,
                    direction: Some(1),
                    part_index: Some(self.pose_part),
                    object_id: Some(object_id),
                }],
            };
            patch.validate()?;
            let key = PackKey {
                namespace: 51,
                id: u64::from(animation_id),
            };
            if let Some(generation) = &self.generation
                && matches!(
                    generation.lookup(key).map_err(|e| e.to_string())?,
                    PackLookup::Record(_)
                )
            {
                return Err("An animation patch for this DID already exists".into());
            }
            let path = source
                .join("animations")
                .join(format!("{animation_id}.toml"));
            if path.exists() {
                return Err("The animation draft filename already exists".into());
            }
            Ok((
                key,
                toml::to_string_pretty(&patch).map_err(|e| e.to_string())?,
                path,
            ))
        })();
        match result {
            Ok((key, text, path)) => {
                self.record = Some(key);
                self.document = text;
                self.document_baseline.clear();
                self.document_path = Some(path);
                self.document_hash = None;
                self.field_form = true;
                self.field_drafts.clear();
                self.review = None;
                self.notice =
                    "New animation part-swap draft. Edit its fields, then save and preview.".into();
            }
            Err(error) => self.notice = error,
        }
    }

    fn run_emote(&mut self) {
        let selected = self.record.is_some_and(|key| key.namespace == 1);
        let source = if selected {
            Ok(self.document.clone())
        } else {
            (|| -> Result<String, String> {
                let id = self
                    .emote_weenie
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let generation = self.generation.as_ref().ok_or("Open a pack generation")?;
                offline_workspace::editable_record(generation, PackKey { namespace: 1, id })
            })()
        };
        let result = (|| -> Result<crate::emote_sandbox::Trace, String> {
            let text = source?;
            let weenie = bace_content_tools::parse(&text).map_err(|e| e.to_string())?;
            let category = self
                .emote_category
                .parse::<u32>()
                .map_err(|e| e.to_string())?;
            crate::emote_sandbox::run(&weenie, category)
        })();
        match result {
            Ok(trace) => {
                self.notice = format!("Emote trace: {} events", trace.lines.len());
                self.emote_trace = Some(trace);
            }
            Err(error) => self.notice = error,
        }
    }

    fn clone_as_new(&mut self) {
        let (Some(old), Some(generation), Some(source)) =
            (self.record, &self.generation, &self.source)
        else {
            self.notice = "Open a record and choose a native TOML folder".into();
            return;
        };
        if self.dirty() {
            self.notice = "Save the current draft before cloning".into();
            return;
        }
        let result = (|| -> Result<(PackKey, String, PathBuf), String> {
            let id = self.new_id.parse::<u32>().map_err(|e| e.to_string())?;
            let key = PackKey {
                namespace: old.namespace,
                id: u64::from(id),
            };
            if matches!(
                generation.lookup(key).map_err(|e| e.to_string())?,
                PackLookup::Record(_)
            ) {
                return Err("The new ID already exists in the pack".into());
            }
            let text = offline_workspace::clone_with_id(&self.document, old.namespace, id)?;
            let folder = offline_workspace::source_folder(old.namespace)
                .ok_or("Unsupported clone namespace")?;
            let path = source.join(folder).join(format!("{id}.toml"));
            if path.exists() {
                return Err("The new draft filename already exists".into());
            }
            Ok((key, text, path))
        })();
        match result {
            Ok((key, text, path)) => {
                self.record = Some(key);
                self.document = text;
                self.document_baseline.clear();
                self.document_path = Some(path);
                self.document_hash = None;
                self.field_drafts.clear();
                self.review = None;
                self.notice = format!(
                    "New {}:{} draft. Edit its fields, then save.",
                    key.namespace, key.id
                );
            }
            Err(error) => self.notice = error,
        }
    }

    fn open_generation(&mut self) {
        if self.dirty() {
            self.notice =
                "Save or discard the current draft before opening another generation".into();
            return;
        }
        let Some(path) = &self.manifest_path else {
            return;
        };
        let result = (|| -> Result<PackGeneration, String> {
            let manifest = load_manifest(path, PackLimits::default()).map_err(|e| e.to_string())?;
            manifest
                .open(
                    path.parent().ok_or("Manifest has no parent folder")?,
                    PackLimits::default(),
                )
                .map_err(|e| e.to_string())
        })();
        match result {
            Ok(generation) => {
                self.generation = Some(generation);
                self.recipe.clear();
                self.after = None;
                self.page.clear();
                self.record = None;
                self.document.clear();
                self.review = None;
                self.notice =
                    "Opened the generation read-only. Only selected records are decoded.".into();
            }
            Err(error) => self.notice = error,
        }
    }

    fn next_page(&mut self) {
        let Some(generation) = &self.generation else {
            return;
        };
        let Ok(namespace) = self.namespace.parse::<u16>() else {
            self.notice = "Choose a content type first".into();
            return;
        };
        if self.after.is_none() {
            self.after = namespace.checked_sub(1).map(|previous| PackKey {
                namespace: previous,
                id: u64::MAX,
            });
        }
        match generation.scan(self.after, 64) {
            Ok(rows) => {
                if rows.is_empty() {
                    self.notice = "End of this content type".into();
                    return;
                }
                self.after = rows.last().map(|(key, _)| *key);
                self.page = rows
                    .into_iter()
                    .filter_map(|(key, value)| {
                        (key.namespace == namespace && matches!(value, PackLookup::Record(_)))
                            .then_some(key)
                    })
                    .collect();
                self.notice = if self.page.is_empty() {
                    "No active records on this page; use Next 64 records to continue".into()
                } else {
                    String::new()
                };
            }
            Err(error) => self.notice = error.to_string(),
        }
    }

    fn open_record(&mut self, key: PackKey) {
        self.open_record_from(key, None);
    }

    fn open_record_from(&mut self, key: PackKey, draft_path: Option<PathBuf>) {
        if self.dirty() {
            self.pending_record = Some((key, draft_path));
            return;
        }
        let Some(generation) = &self.generation else {
            return;
        };
        let loaded = (|| -> Result<LoadedDraft, String> {
            let canonical = self.source.as_ref().and_then(|source| {
                offline_workspace::source_folder(key.namespace)
                    .map(|folder| source.join(folder).join(format!("{}.toml", key.id)))
            });
            let explicit_draft = draft_path.is_some();
            let path = draft_path.or(canonical);
            if let Some(path) = &path {
                match fs::symlink_metadata(path) {
                    Ok(metadata) => {
                        if !metadata.file_type().is_file() || metadata.len() > 16 * 1024 * 1024 {
                            return Err(
                                "Draft must be a regular TOML file of at most 16 MiB".into()
                            );
                        }
                        let bytes = fs::read(path).map_err(|e| e.to_string())?;
                        let text = String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?;
                        let folder = offline_workspace::source_folder(key.namespace)
                            .ok_or("Derived indexes are read-only")?;
                        if offline_workspace::compile_draft(folder, &text)?.key != key {
                            return Err("Draft identity differs from its selected record".into());
                        }
                        return Ok(LoadedDraft {
                            text,
                            path: Some(path.clone()),
                            hash: Some(Sha256::digest(bytes).into()),
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        if explicit_draft {
                            return Err("Saved draft moved; refresh recipe parts".into());
                        }
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            Ok(LoadedDraft {
                text: offline_workspace::editable_record(generation, key)?,
                path,
                hash: None,
            })
        })();
        match loaded {
            Ok(LoadedDraft { text, path, hash }) => {
                self.record = Some(key);
                self.document = text;
                self.document_path = path;
                self.document_hash = hash;
                self.field_drafts.clear();
                self.field_form = true;
                if self.namespace.parse::<u16>().ok() != Some(key.namespace) {
                    self.after = None;
                    self.page.clear();
                }
                self.namespace = key.namespace.to_string();
                self.id = key.id.to_string();
                self.document_baseline = self.document.clone();
                self.notice = format!("Loaded {}:{} into an editable draft", key.namespace, key.id);
            }
            Err(error) => self.notice = error,
        }
    }

    fn save_draft(&mut self) {
        let Some(key) = self.record else {
            return;
        };
        let Some(folder) = offline_workspace::source_folder(key.namespace) else {
            return;
        };
        let Some(source) = &self.source else {
            self.notice = "Choose a native content folder".into();
            return;
        };
        let record = match offline_workspace::compile_draft(folder, &self.document) {
            Ok(record) if record.key == key => record,
            Ok(_) => {
                self.notice = "Draft identity changed. Use its original namespace and ID.".into();
                return;
            }
            Err(error) => {
                self.notice = error;
                return;
            }
        };
        let _ = record;
        let path = self
            .document_path
            .clone()
            .unwrap_or_else(|| source.join(folder).join(format!("{}.toml", key.id)));
        let result = (|| -> Result<(), String> {
            fs::create_dir_all(path.parent().ok_or("Draft has no parent folder")?)
                .map_err(|e| e.to_string())?;
            let current = match fs::read(&path) {
                Ok(bytes) => Some(<[u8; 32]>::from(Sha256::digest(bytes))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            if current != self.document_hash {
                return Err("Draft changed on disk; reopen it before saving".into());
            }
            let mut temp = tempfile::NamedTempFile::new_in(path.parent().expect("parent checked"))
                .map_err(|e| e.to_string())?;
            temp.write_all(self.document.as_bytes())
                .map_err(|e| e.to_string())?;
            temp.as_file().sync_all().map_err(|e| e.to_string())?;
            if current.is_some() {
                temp.persist(&path).map_err(|e| e.to_string())?;
            } else {
                temp.persist_noclobber(&path).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.recipe.clear();
                self.document_hash = Some(Sha256::digest(self.document.as_bytes()).into());
                self.document_path = Some(path);
                self.document_baseline = self.document.clone();
                self.review = None;
                self.notice = "Saved native TOML draft. Review changes before building.".into();
            }
            Err(error) => self.notice = error,
        }
    }

    fn stage_removal(&mut self) {
        let (Some(key), Some(source)) = (self.record, &self.source) else {
            return;
        };
        let kind = if key.namespace == 1 {
            Some("weenie")
        } else if key.namespace == 46 {
            Some("loot")
        } else if key.namespace == 47 {
            Some("rare")
        } else if key.namespace == 50 {
            Some("clothing")
        } else if key.namespace == 51 {
            Some("animation")
        } else {
            offline_workspace::world_kind(key.namespace)
        };
        let Some(kind) = kind else {
            return;
        };
        let folder = source.join("removals");
        let result = (|| -> Result<PathBuf, String> {
            fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let path = folder.join(format!("{}-{}.toml", kind, key.id));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            writeln!(file, "kind = {kind:?}\nid = {}", key.id).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            Ok(path)
        })();
        self.notice = match result {
            Ok(path) => {
                self.review = None;
                self.recipe.clear();
                format!(
                    "Staged removal at {}. Review before building.",
                    path.display()
                )
            }
            Err(error) => error,
        };
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some((key, draft_path)) = self.pending_record.clone() {
            egui::Window::new("Unsaved offline draft")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(
                        "Save your current draft before opening another record, or discard it.",
                    );
                    if ui.button("Keep current draft").clicked() {
                        self.pending_record = None;
                    }
                    if ui.button("Discard and open record").clicked() {
                        self.pending_record = None;
                        self.document_baseline = self.document.clone();
                        self.open_record_from(key, draft_path);
                    }
                });
        }
        ui.label("Open a runtime generation, copy individual records into editable TOML, then review and build an immutable candidate. No database is needed.");
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.dirty(), egui::Button::new("Choose .manifest…"))
                .clicked()
            {
                self.manifest_path = rfd::FileDialog::new()
                    .add_filter("BACE generation", &["manifest"])
                    .pick_file();
                self.generation = None;
                self.review = None;
                self.recipe.clear();
            }
            ui.label(
                self.manifest_path
                    .as_ref()
                    .map_or("No generation selected".into(), |p| p.display().to_string()),
            );
            if ui
                .add_enabled(
                    self.manifest_path.is_some(),
                    egui::Button::new("Open read-only"),
                )
                .clicked()
            {
                self.open_generation();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.dirty(),
                    egui::Button::new("Choose native TOML folder…"),
                )
                .clicked()
            {
                self.source = rfd::FileDialog::new().pick_folder();
                self.review = None;
                self.recipe.clear();
            }
            ui.label(
                self.source
                    .as_ref()
                    .map_or("No draft folder selected".into(), |p| {
                        p.display().to_string()
                    }),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("Choose candidate output folder…").clicked() {
                self.output = rfd::FileDialog::new().pick_folder();
            }
            ui.label(
                self.output
                    .as_ref()
                    .map_or("No output folder selected".into(), |p| {
                        p.display().to_string()
                    }),
            );
        });
        ui.separator();
        ui.heading("Browse and edit");
        ui.horizontal(|ui| {
            ui.label("Content type");
            let chosen = self.namespace.parse::<u16>().ok();
            let label = chosen
                .and_then(content_type_label)
                .unwrap_or("Choose a content type");
            egui::ComboBox::from_id_salt("offline-content-type")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for namespace in std::iter::once(1).chain(16..=45).chain([46, 47, 50, 51]) {
                        if ui
                            .selectable_label(
                                chosen == Some(namespace),
                                content_type_label(namespace).unwrap_or("Unknown"),
                            )
                            .clicked()
                        {
                            self.namespace = namespace.to_string();
                            self.after = None;
                            self.page.clear();
                        }
                    }
                });
            ui.label("ID");
            ui.add(egui::TextEdit::singleline(&mut self.id).desired_width(140.0));
            if ui
                .add_enabled(self.generation.is_some(), egui::Button::new("Open record"))
                .clicked()
            {
                match (self.namespace.parse::<u16>(), self.id.parse::<u64>()) {
                    (Ok(namespace), Ok(id)) => self.open_record(PackKey { namespace, id }),
                    _ => self.notice = "Enter numeric namespace and ID".into(),
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.generation.is_some(),
                    egui::Button::new("Browse from start"),
                )
                .clicked()
            {
                let Ok(namespace) = self.namespace.parse::<u16>() else {
                    self.notice = "Choose a content type first".into();
                    return;
                };
                self.after = namespace.checked_sub(1).map(|previous| PackKey {
                    namespace: previous,
                    id: u64::MAX,
                });
                self.next_page();
            }
            if ui
                .add_enabled(
                    self.generation.is_some(),
                    egui::Button::new("Next 64 records"),
                )
                .clicked()
            {
                self.next_page();
            }
        });
        let mut selected = None;
        egui::ScrollArea::vertical()
            .max_height(130.0)
            .show(ui, |ui| {
                for key in &self.page {
                    if ui
                        .selectable_label(
                            self.record == Some(*key),
                            format!("{}:{}", key.namespace, key.id),
                        )
                        .clicked()
                    {
                        selected = Some(*key);
                    }
                }
            });
        if let Some(key) = selected {
            self.open_record(key);
        }
        if let Some(key) = self.record {
            ui.label(format!("Editing {}:{}", key.namespace, key.id));
            ui.horizontal(|ui| {
                if ui.selectable_label(self.field_form, "Fields").clicked() {
                    self.field_form = true;
                    self.field_drafts.clear();
                }
                if ui
                    .selectable_label(!self.field_form, "TOML source")
                    .clicked()
                {
                    self.field_form = false;
                    self.field_drafts.clear();
                }
            });
            if self.dirty() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Unsaved draft: save it before reviewing a candidate.",
                );
            }
            if self.field_form {
                match toml::from_str::<toml::Value>(&self.document) {
                    Ok(mut value) => {
                        let mut changed = crate::generic_form::edit(
                            ui,
                            &mut value,
                            "document",
                            &mut self.field_drafts,
                        );
                        changed |= crate::generic_form::optional_world_messages(
                            ui,
                            key.namespace,
                            &mut value,
                        );
                        if changed {
                            match toml::to_string_pretty(&value) {
                                Ok(text) => {
                                    self.document = text;
                                    self.review = None;
                                    self.field_drafts.clear();
                                }
                                Err(error) => self.notice = error.to_string(),
                            }
                        }
                    }
                    Err(error) => {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("TOML needs correction: {error}"),
                        );
                        self.field_form = false;
                    }
                }
            } else if ui
                .add(
                    egui::TextEdit::multiline(&mut self.document)
                        .desired_rows(18)
                        .code_editor(),
                )
                .changed()
            {
                self.review = None;
            }
            ui.horizontal(|ui| {
                if ui.button("Validate and save TOML draft").clicked() {
                    self.save_draft();
                }
                if ui.button("Stage removal").clicked() {
                    self.stage_removal();
                }
            });
            ui.horizontal(|ui| {
                ui.label("New ID");
                ui.add(egui::TextEdit::singleline(&mut self.new_id).desired_width(100.0));
                if ui.button("Duplicate as new content").clicked() {
                    self.clone_as_new();
                }
            });
            ui.small("The opened .bace file remains read-only. A staged removal is a TOML file you can delete before publication.");
        }
        ui.separator();
        if let Some((key, draft_path)) = self.recipe.ui(
            ui,
            self.manifest_path.as_deref(),
            self.source.as_deref(),
            self.record,
        ) {
            self.open_record_from(key, draft_path);
        }
        ui.separator();
        ui.heading("Visual part-swap preview");
        ui.small("Choose an installed Portal DAT and a Setup/Animation pair. The current ClothingBase or animation draft is applied without changing the DAT.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Choose client_portal.dat…").clicked() {
                self.portal = rfd::FileDialog::new()
                    .add_filter("Asheron's Call DAT", &["dat"])
                    .pick_file();
            }
            ui.label(
                self.portal
                    .as_ref()
                    .map_or("No Portal DAT selected".into(), |p| p.display().to_string()),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Setup DID");
            ui.add(
                egui::TextEdit::singleline(&mut self.pose_setup)
                    .desired_width(100.0)
                    .hint_text("02000001"),
            );
            ui.label("Animation DID");
            ui.add(
                egui::TextEdit::singleline(&mut self.pose_animation)
                    .desired_width(100.0)
                    .hint_text("03000001"),
            );
            ui.label("Frame");
            ui.add(egui::DragValue::new(&mut self.pose_frame).range(0..=65_535));
            if ui
                .add_enabled(
                    self.portal.is_some() && self.pose_job.is_none(),
                    egui::Button::new("Load pose"),
                )
                .clicked()
            {
                self.load_pose();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Replacement GfxObj DID");
            ui.add(
                egui::TextEdit::singleline(&mut self.pose_object)
                    .desired_width(100.0)
                    .hint_text("01000001"),
            );
            ui.label("Part");
            ui.add(egui::DragValue::new(&mut self.pose_part).range(0..=255));
            if ui.button("New animation swap draft").clicked() {
                self.new_animation_patch();
            }
        });
        if self.pose_job.is_some() {
            ui.spinner();
        }
        if let Some(texture) = &self.pose_texture {
            let side = ui.available_width().min(512.0);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click_and_drag());
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let mut changed = false;
            if response.dragged() {
                let delta = ui.input(|input| input.pointer.delta());
                self.pose_camera.yaw += delta.x * 0.01;
                self.pose_camera.pitch = (self.pose_camera.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
                changed = true;
            }
            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    self.pose_camera.zoom =
                        (self.pose_camera.zoom * (scroll * 0.002).exp()).clamp(0.2, 4.0);
                    changed = true;
                }
            }
            if changed {
                self.raster_pose(ui.ctx());
            }
            if let Some(scene) = &self.pose_scene {
                egui::ScrollArea::vertical()
                    .max_height(130.0)
                    .show(ui, |ui| {
                        for line in &scene.report {
                            ui.monospace(line);
                        }
                    });
            }
        }
        ui.separator();
        ui.heading("Emote table test");
        ui.small("Run a native weenie emote against a disposable local test actor. Supported changes appear in the trace; unsupported live effects stop explicitly.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Weenie ID");
            ui.add(
                egui::TextEdit::singleline(&mut self.emote_weenie)
                    .desired_width(90.0)
                    .hint_text("current draft or ID"),
            );
            ui.label("Category");
            ui.add(
                egui::TextEdit::singleline(&mut self.emote_category)
                    .desired_width(70.0)
                    .hint_text("7"),
            );
            if ui.button("Run emote").clicked() {
                self.run_emote();
            }
        });
        if let Some(trace) = &self.emote_trace {
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for line in &trace.lines {
                        ui.monospace(line);
                    }
                });
            ui.small(format!(
                "XP: {} · {} quest entries · {} inventory entries",
                trace.xp,
                trace.quests.len(),
                trace.inventory.len()
            ));
        }
        ui.separator();
        ui.heading("Review candidate");
        if ui
            .add_enabled(
                self.manifest_path.is_some()
                    && self.source.is_some()
                    && !self.busy()
                    && !self.dirty(),
                egui::Button::new("Review changes"),
            )
            .clicked()
            && let (Some(manifest), Some(source)) = (&self.manifest_path, &self.source)
        {
            let manifest = manifest.clone();
            let source = source.clone();
            let (sender, receiver) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("studio-candidate-review".into())
                .spawn(move || {
                    let _ = sender.send(offline_workspace::review(&manifest, &source));
                }) {
                Ok(thread) => {
                    self.review_job = Some(ReviewJob { receiver, thread });
                    self.notice = "Reviewing native drafts…".into();
                }
                Err(error) => self.notice = error.to_string(),
            }
        }
        if self.review_job.is_some() {
            ui.spinner();
        }
        if let Some(review) = &self.review {
            ui.label(format!(
                "{} changes against generation {}",
                review.changes.len(),
                review.manifest.generation
            ));
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for change in &review.changes {
                        ui.label(format!(
                            "{}  {}:{}  {}",
                            change.action, change.key.namespace, change.key.id, change.source
                        ));
                    }
                });
            ui.checkbox(
                &mut self.confirmed,
                "I reviewed these additions, replacements, and removals",
            );
            if review.needs_compaction() {
                ui.checkbox(&mut self.confirm_compaction, "I approve creating a compacted candidate base because this generation already has two deltas");
            }
            let ready = self.confirmed
                && (!review.needs_compaction() || self.confirm_compaction)
                && self.output.is_some()
                && self.job.is_none()
                && !review.changes.is_empty();
            if ui
                .add_enabled(ready, egui::Button::new("Build candidate .bace"))
                .clicked()
            {
                let review = review.clone();
                let output = self.output.clone().expect("enabled output");
                let compaction = self.confirm_compaction;
                let (sender, receiver) = mpsc::sync_channel(1);
                match std::thread::Builder::new()
                    .name("studio-candidate-build".into())
                    .spawn(move || {
                        let result = offline_workspace::build(&review, &output, compaction);
                        let _ = sender.send(result);
                    }) {
                    Ok(thread) => {
                        self.job = Some(BuildJob { receiver, thread });
                        self.notice = "Building candidate…".into();
                    }
                    Err(error) => self.notice = error.to_string(),
                }
            }
        }
        if self.job.is_some() {
            ui.spinner();
        }
        crate::theme::notice(ui, &self.notice);
        ui.small("A candidate is local only. The server inbox can review supported native TOML; animation swaps remain offline only until runtime motion overlay support is complete.");
    }
}
