use crate::theme;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::thread::JoinHandle;

#[derive(Default)]
pub(crate) struct PackBuilder {
    source: Option<PathBuf>,
    output: Option<PathBuf>,
    job: Option<BuildJob>,
    notice: String,
    close_after: bool,
}
struct BuildJob {
    receiver: Receiver<Result<PathBuf, String>>,
    thread: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for BuildJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl PackBuilder {
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &mut self.job {
            if ctx.input(|i| i.viewport().close_requested()) {
                job.cancel.store(true, Ordering::Relaxed);
                self.close_after = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if job.thread.as_ref().is_some_and(|t| t.is_finished()) {
                let panicked = job.thread.take().is_some_and(|t| t.join().is_err());
                self.notice = if panicked {
                    "Pack worker stopped unexpectedly.".into()
                } else {
                    match job.receiver.try_recv() {
                        Ok(Ok(path)) => format!(
                            "Built immutable pack and manifest in {}. This is not yet activated on a server.",
                            path.display()
                        ),
                        Ok(Err(e)) => e,
                        Err(e) => e.to_string(),
                    }
                };
                self.job = None;
                if self.close_after {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        theme::subtitle(
            ui,
            "Compile a native content folder into an immutable .bace pack and generation manifest.",
        );
        ui.add_space(12.0);
        ui.add_enabled_ui(self.job.is_none(),|ui| {
            theme::card().show(ui,|ui| {
            ui.set_min_width(ui.available_width());
            theme::eyebrow(ui,"01  SOURCE CONTENT");
            ui.heading("Native TOML folder");
            if ui.button("Choose native TOML folder…").clicked() && let Some(path)=rfd::FileDialog::new().pick_folder() {self.source=Some(path);}
            if let Some(path)=&self.source {ui.label(path.display().to_string());}
            ui.small("Includes TOML in subfolders; skips conversion manifest.toml files. Duplicate weenie IDs reject the build.");
            });
            ui.add_space(14.0);
            theme::card().show(ui,|ui| {
            ui.set_min_width(ui.available_width());
            theme::eyebrow(ui,"02  DESTINATION");
            ui.heading("Build output");
            ui.label("Each build creates a new folder with a pack and its manifest.");
            if ui.button("Choose pack output folder…").clicked() && let Some(path)=rfd::FileDialog::new().pick_folder() {self.output=Some(path);}
            if let Some(path)=&self.output {ui.label(path.display().to_string());}
            });
            ui.add_space(18.0);
            if ui.add_enabled(self.source.is_some()&&self.output.is_some(),theme::primary("Build .bace pack")).clicked()
                && let (Some(source),Some(output))=(self.source.clone(),self.output.clone()) {
                let (sender,receiver)=mpsc::sync_channel(1);
                let cancel=Arc::new(AtomicBool::new(false));let flag=Arc::clone(&cancel);let ctx=ui.ctx().clone();
                match std::thread::Builder::new().name("content-pack-build".into()).spawn(move||{
                    let result=build_folder(&source,&output,&flag);
                    let _=sender.send(result);ctx.request_repaint();
                }) {
                    Ok(thread)=>{self.job=Some(BuildJob {receiver,thread:Some(thread),cancel});self.notice="Building…".into();}
                    Err(e)=>self.notice=e.to_string(),
                }
            }
        });
        if let Some(job) = &self.job {
            ui.spinner();
            if ui.button("Cancel build").clicked() {
                job.cancel.store(true, Ordering::Relaxed);
                self.notice = "Cancelling after the current record…".into();
            }
        }
        theme::notice(ui, &self.notice);
        ui.add_space(14.0);
        ui.collapsing("Memory use & runtime status",|ui| {
            ui.label("The builder reads one weenie at a time. Payloads are spooled to disk; only the compact ID/offset index remains in memory.");
            ui.label("Runtime mmap readers exist. The content-worker still uses an in-memory catalog; pack activation and a bounded gameplay cache remain separate integration work.");
        });
    }
}

fn build_folder(source: &Path, output: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    if !source.is_dir() || !output.is_dir() {
        return Err("Choose existing source and output folders.".into());
    }
    let mut paths = Vec::new();
    let mut directories = vec![(source.to_path_buf(), 0)];
    let mut entries = 0_usize;
    while let Some((directory, depth)) = directories.pop() {
        if depth > 16 {
            return Err("Source directory nesting exceeds 16 levels.".into());
        }
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            if cancel.load(Ordering::Relaxed) {
                return Err("Pack build cancelled.".into());
            }
            entries += 1;
            if entries > 200_000 {
                return Err("Source folder exceeds 200,000 entries.".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            let path = entry.path();
            if kind.is_symlink() {
                return Err(format!(
                    "Symlinks are not accepted in pack sources: {}",
                    path.display()
                ));
            }
            if kind.is_dir() {
                directories.push((path, depth + 1));
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("toml"))
                && path.file_name().is_some_and(|n| n != "manifest.toml")
            {
                paths.push(path);
                if paths.len() > 100_000 {
                    return Err("A pack build accepts at most 100,000 sources.".into());
                }
            }
        }
    }
    let directory = tempfile::Builder::new()
        .prefix("weenie-pack-")
        .tempdir_in(output)
        .map_err(|e| e.to_string())?;
    bace_content_tools::build_weenie_pack(&paths, directory.path(), cancel)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Pack build cancelled; unpublished output removed.".into());
    }
    Ok(directory.keep())
}
