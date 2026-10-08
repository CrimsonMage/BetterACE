//! Recipe-focused navigation over frozen world rows in a mapped generation.
use crate::offline_workspace;
use bace_content::WorldRecordV1;
use bace_storage_codec::{PackKey, PackLimits, PackLookup, load_manifest};
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

const MAX_SCANNED: usize = 250_000;
const MAX_LINKS: usize = 4096;

#[derive(Clone, Debug)]
pub(crate) struct RecipeLink {
    pub key: PackKey,
    pub group: &'static str,
    pub label: String,
    pub draft_path: Option<PathBuf>,
}

fn add_link(
    links: &mut Vec<RecipeLink>,
    key: PackKey,
    group: &'static str,
    label: String,
) -> Result<(), String> {
    if links.len() >= MAX_LINKS {
        return Err("Recipe has more than 4096 linked rows".into());
    }
    links.push(RecipeLink {
        key,
        group,
        label,
        draft_path: None,
    });
    Ok(())
}

fn include_row(
    links: &mut Vec<RecipeLink>,
    modifiers: &mut BTreeSet<u32>,
    recipe_id: u32,
    key: PackKey,
    bytes: Option<&[u8]>,
) -> Result<(), String> {
    let Some(bytes) = bytes else { return Ok(()) };
    let row = bace_content_tools::decode_world_record(bytes)?;
    if row.namespace() != key.namespace || row.id() != key.id {
        return Err("Recipe link row identity mismatch".into());
    }
    match row {
        WorldRecordV1::CookBook(r) if r.recipe_id == recipe_id => add_link(
            links,
            key,
            "Ways to use this recipe",
            format!("{} + {} → recipe", r.source_w_c_i_d, r.target_w_c_i_d),
        )?,
        WorldRecordV1::RecipeMod(r) if r.recipe_id == recipe_id => {
            modifiers.insert(r.id);
            add_link(
                links,
                key,
                "Success and failure effects",
                format!(
                    "Effect {} · {}",
                    r.id,
                    if r.executes_on_success {
                        "success"
                    } else {
                        "failure"
                    }
                ),
            )?;
        }
        WorldRecordV1::RecipeModsBool(r) if modifiers.contains(&r.recipe_mod_id) => {
            modifier_link(links, key, "Boolean", r.recipe_mod_id, r.index, r.stat)?;
        }
        WorldRecordV1::RecipeModsDID(r) if modifiers.contains(&r.recipe_mod_id) => {
            modifier_link(links, key, "Asset ID", r.recipe_mod_id, r.index, r.stat)?;
        }
        WorldRecordV1::RecipeModsFloat(r) if modifiers.contains(&r.recipe_mod_id) => {
            modifier_link(links, key, "Decimal", r.recipe_mod_id, r.index, r.stat)?;
        }
        WorldRecordV1::RecipeModsIID(r) if modifiers.contains(&r.recipe_mod_id) => {
            modifier_link(links, key, "Object ID", r.recipe_mod_id, r.index, r.stat)?;
        }
        WorldRecordV1::RecipeModsInt(r) if modifiers.contains(&r.recipe_mod_id) => {
            modifier_link(links, key, "Number", r.recipe_mod_id, r.index, r.stat)?;
        }
        WorldRecordV1::RecipeModsString(r) if modifiers.contains(&r.recipe_mod_id) => {
            modifier_link(links, key, "Text", r.recipe_mod_id, r.index, r.stat)?;
        }
        WorldRecordV1::RecipeRequirementsBool(r) if r.recipe_id == recipe_id => {
            requirement_link(links, key, "Boolean", r.index, r.stat)?;
        }
        WorldRecordV1::RecipeRequirementsDID(r) if r.recipe_id == recipe_id => {
            requirement_link(links, key, "Asset ID", r.index, r.stat)?;
        }
        WorldRecordV1::RecipeRequirementsFloat(r) if r.recipe_id == recipe_id => {
            requirement_link(links, key, "Decimal", r.index, r.stat)?;
        }
        WorldRecordV1::RecipeRequirementsIID(r) if r.recipe_id == recipe_id => {
            requirement_link(links, key, "Object ID", r.index, r.stat)?;
        }
        WorldRecordV1::RecipeRequirementsInt(r) if r.recipe_id == recipe_id => {
            requirement_link(links, key, "Number", r.index, r.stat)?;
        }
        WorldRecordV1::RecipeRequirementsString(r) if r.recipe_id == recipe_id => {
            requirement_link(links, key, "Text", r.index, r.stat)?;
        }
        _ => {}
    }
    Ok(())
}

fn modifier_link(
    links: &mut Vec<RecipeLink>,
    key: PackKey,
    kind: &str,
    parent: u32,
    index: i8,
    stat: i32,
) -> Result<(), String> {
    add_link(
        links,
        key,
        "Effect details",
        format!("{kind} · effect {parent} · slot {index} · stat {stat}"),
    )
}

fn requirement_link(
    links: &mut Vec<RecipeLink>,
    key: PackKey,
    kind: &str,
    index: i8,
    stat: i32,
) -> Result<(), String> {
    add_link(
        links,
        key,
        "Requirements",
        format!("{kind} · slot {index} · stat {stat}"),
    )
}

pub(crate) fn find_links(
    manifest_path: &Path,
    source: Option<&Path>,
    recipe_id: u32,
) -> Result<Vec<RecipeLink>, String> {
    let limits = PackLimits::default();
    let manifest = load_manifest(manifest_path, limits).map_err(|e| e.to_string())?;
    let generation = manifest
        .open(
            manifest_path
                .parent()
                .ok_or("Manifest has no parent folder")?,
            limits,
        )
        .map_err(|e| e.to_string())?;
    let mut overlay = BTreeMap::<PackKey, Option<Vec<u8>>>::new();
    let mut draft_paths = BTreeMap::<PackKey, PathBuf>::new();
    if let Some(source) = source {
        let review = offline_workspace::review(manifest_path, source)?;
        for change in &review.changes {
            if change.source != "generated index" {
                draft_paths.insert(change.key, source.join(&change.source));
            }
        }
        for record in review.changed_records() {
            if record.key.namespace == 16 || (24..=37).contains(&record.key.namespace) {
                overlay.insert(record.key, record.value.clone());
            }
        }
    }
    let recipe_key = PackKey {
        namespace: 24,
        id: u64::from(recipe_id),
    };
    let exists = match overlay.get(&recipe_key) {
        Some(changed) => changed.is_some(),
        None => matches!(
            generation.lookup(recipe_key).map_err(|e| e.to_string())?,
            PackLookup::Record(_)
        ),
    };
    if !exists {
        return Err("Recipe ID is absent from the opened generation".into());
    }
    let mut links = vec![RecipeLink {
        key: recipe_key,
        group: "Recipe",
        label: format!("Recipe {recipe_id} · outputs, skill and success/failure chances"),
        draft_path: None,
    }];
    let mut modifiers = BTreeSet::new();
    // Draft additions can define a modifier that existing effect-detail rows reference.
    for (key, bytes) in &overlay {
        if key.namespace == 25
            && let Some(bytes) = bytes
            && let WorldRecordV1::RecipeMod(row) = bace_content_tools::decode_world_record(bytes)?
            && row.recipe_id == recipe_id
        {
            modifiers.insert(row.id);
        }
    }
    let mut seen = BTreeSet::new();
    let mut scanned = 0usize;
    for (start, end) in [(16_u16, 16_u16), (25, 37)] {
        let mut cursor = Some(PackKey {
            namespace: start - 1,
            id: u64::MAX,
        });
        loop {
            let page = generation.scan(cursor, 256).map_err(|e| e.to_string())?;
            if page.is_empty() {
                break;
            }
            cursor = page.last().map(|(key, _)| *key);
            let mut beyond = false;
            for (key, found) in page {
                if key.namespace > end {
                    beyond = true;
                    break;
                }
                if key.namespace < start {
                    continue;
                }
                scanned += 1;
                if scanned > MAX_SCANNED {
                    return Err("Recipe link scan exceeds 250,000 rows".into());
                }
                if let Some(changed) = overlay.get(&key) {
                    seen.insert(key);
                    include_row(
                        &mut links,
                        &mut modifiers,
                        recipe_id,
                        key,
                        changed.as_deref(),
                    )?;
                } else if let PackLookup::Record(record) = found {
                    include_row(
                        &mut links,
                        &mut modifiers,
                        recipe_id,
                        key,
                        Some(record.bytes()),
                    )?;
                }
            }
            if beyond {
                break;
            }
        }
    }
    for (key, changed) in overlay {
        if !seen.contains(&key) {
            include_row(
                &mut links,
                &mut modifiers,
                recipe_id,
                key,
                changed.as_deref(),
            )?;
        }
    }
    links.sort_by_key(|link| (link.group, link.key));
    for link in &mut links {
        link.draft_path = draft_paths.get(&link.key).cloned();
    }
    Ok(links)
}

#[derive(Default)]
pub(crate) struct RecipeWorkspace {
    recipe_id: String,
    selected_recipe: Option<u32>,
    links: Vec<RecipeLink>,
    job: Option<RecipeJob>,
    notice: String,
}

struct RecipeJob {
    receiver: Receiver<Result<Vec<RecipeLink>, String>>,
    thread: JoinHandle<()>,
}

impl RecipeWorkspace {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn clear(&mut self) {
        self.links.clear();
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if self
            .job
            .as_ref()
            .is_some_and(|job| job.thread.is_finished())
        {
            let job = self.job.take().expect("finished recipe scan");
            self.notice = if job.thread.join().is_err() {
                "Recipe search stopped unexpectedly".into()
            } else {
                match job.receiver.try_recv() {
                    Ok(Ok(links)) => {
                        self.notice = format!("Found {} recipe and related rows", links.len());
                        self.links = links;
                        return;
                    }
                    Ok(Err(error)) => error,
                    Err(error) => error.to_string(),
                }
            };
        } else if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        manifest: Option<&Path>,
        source: Option<&Path>,
        selected: Option<PackKey>,
    ) -> Option<(PackKey, Option<PathBuf>)> {
        if let Some(PackKey { namespace: 24, id }) = selected
            && self.selected_recipe != u32::try_from(id).ok()
        {
            self.selected_recipe = u32::try_from(id).ok();
            self.recipe_id = id.to_string();
            self.links.clear();
        }
        let mut open = None;
        egui::CollapsingHeader::new("Recipe editor")
            .default_open(true)
            .show(ui, |ui| {
            ui.label("Open a recipe and its uses, requirements and effects together. Each row saves as a separate native TOML draft.");
            ui.horizontal(|ui| {
                ui.label("Recipe ID");
                ui.add(egui::TextEdit::singleline(&mut self.recipe_id).desired_width(120.0));
                if ui.add_enabled(manifest.is_some() && self.job.is_none(), egui::Button::new("Find recipe parts")).clicked() {
                    match (manifest, self.recipe_id.parse::<u32>()) {
                        (Some(manifest), Ok(id)) => {
                            let manifest = manifest.to_owned();
                            let source: Option<PathBuf> = source.map(Path::to_owned);
                            let (sender, receiver) = mpsc::sync_channel(1);
                            match std::thread::Builder::new().name("studio-recipe-links".into()).spawn(move || {
                                let _ = sender.send(find_links(&manifest, source.as_deref(), id));
                            }) {
                                Ok(thread) => {
                                    self.links.clear();
                                    self.job = Some(RecipeJob { receiver, thread });
                                    self.notice = "Finding linked recipe rows…".into();
                                }
                                Err(error) => self.notice = error.to_string(),
                            }
                        }
                        _ => self.notice = "Enter a numeric recipe ID and open a generation".into(),
                    }
                }
            });
            if self.job.is_some() { ui.spinner(); }
            for group in ["Recipe", "Ways to use this recipe", "Requirements", "Success and failure effects", "Effect details"] {
                let rows: Vec<_> = self.links.iter().filter(|link| link.group == group).collect();
                if rows.is_empty() { continue; }
                ui.strong(format!("{group} ({})", rows.len()));
                for link in rows {
                    if ui.button(&link.label).clicked() { open = Some((link.key, link.draft_path.clone())); }
                }
            }
            if !self.notice.is_empty() { ui.label(&self.notice); }
        });
        open
    }
}
