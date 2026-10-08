//! Bounded ClothingBase document I/O, separate from native weenie files.
use bace_content::ClothingPatchV1;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::mpsc, thread::JoinHandle};
pub(crate) struct ClothingDocument {
    pub patch: ClothingPatchV1,
    pub saved: Option<ClothingPatchV1>,
    pub path: Option<PathBuf>,
    pub hash: Option<[u8; 32]>,
}
impl ClothingDocument {
    pub fn new(patch: ClothingPatchV1) -> Self {
        Self {
            patch,
            saved: None,
            path: None,
            hash: None,
        }
    }
    pub fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.patch)
    }
    pub fn text(&self) -> Result<String, String> {
        native_text(&self.patch)
    }

    /// Copy a complete selected entry; retain the destination ID and unrelated entries.
    pub fn copy_entries(
        &mut self,
        setup: Option<bace_content::ClothingSetup>,
        palette: Option<bace_content::ClothingPalette>,
    ) -> Result<ClothingPatchV1, String> {
        let incoming = ClothingPatchV1 {
            schema_version: 1,
            id: self.patch.id,
            setups: setup.into_iter().collect(),
            palettes: palette.into_iter().collect(),
        };
        let candidate = bace_content::resolve_clothing(Some(&self.patch), &incoming)
            .map_err(|e| e.to_string())?;
        native_text(&candidate)?;
        Ok(std::mem::replace(&mut self.patch, candidate))
    }
}
pub(crate) fn native_text(patch: &ClothingPatchV1) -> Result<String, String> {
    patch.validate().map_err(|e| e.to_string())?;
    let text = toml::to_string_pretty(patch).map_err(|e| e.to_string())?;
    if text.len() > bace_import::MAX_CLOTHING_BYTES {
        return Err("ClothingBase exceeds 1 MiB".into());
    }
    Ok(text)
}
pub(crate) fn native_parse(text: &str) -> Result<ClothingPatchV1, String> {
    if text.len() > bace_import::MAX_CLOTHING_BYTES {
        return Err("ClothingBase exceeds 1 MiB".into());
    }
    let patch: ClothingPatchV1 = toml::from_str(text).map_err(|e| e.to_string())?;
    patch.validate().map_err(|e| e.to_string())?;
    Ok(patch)
}
pub(crate) enum Request {
    Open(PathBuf),
    Write {
        path: PathBuf,
        text: String,
        expected: Option<[u8; 32]>,
        native: bool,
    },
}
pub(crate) enum Reply {
    Opened(ClothingDocument),
    Written {
        path: PathBuf,
        hash: [u8; 32],
        native: bool,
    },
}
pub(crate) struct Job {
    thread: Option<JoinHandle<()>>,
    receiver: mpsc::Receiver<Result<Reply, String>>,
}
impl Job {
    pub fn start(request: Request, ctx: eframe::egui::Context) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("clothing-document-io".into())
            .spawn(move || {
                let result = (|| match request {
                    Request::Open(path) => {
                        use std::io::Read;
                        let mut text = String::new();
                        std::fs::File::open(&path)
                            .map_err(|e| e.to_string())?
                            .take((bace_import::MAX_CLOTHING_BYTES + 1) as u64)
                            .read_to_string(&mut text)
                            .map_err(|e| e.to_string())?;
                        if text.len() > bace_import::MAX_CLOTHING_BYTES {
                            return Err("ClothingBase exceeds 1 MiB".into());
                        }
                        let json = path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
                        let patch = if json {
                            bace_import::import_clothing_json(&text)?
                        } else {
                            native_parse(&text)?
                        };
                        let mut doc = ClothingDocument::new(patch);
                        if !json {
                            doc.saved = Some(doc.patch.clone());
                            doc.path = Some(path);
                            doc.hash = Some(Sha256::digest(text.as_bytes()).into());
                        }
                        Ok(Reply::Opened(doc))
                    }
                    Request::Write {
                        path,
                        text,
                        expected,
                        native,
                    } => {
                        if text.len() > bace_import::MAX_CLOTHING_BYTES {
                            return Err("ClothingBase exceeds 1 MiB".into());
                        }
                        crate::document::write(&path, &text, expected)?;
                        Ok(Reply::Written {
                            path,
                            hash: Sha256::digest(text.as_bytes()).into(),
                            native,
                        })
                    }
                })();
                let _ = sender.send(result);
                ctx.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            thread: Some(thread),
            receiver,
        })
    }
    pub fn poll(&mut self) -> Option<Result<Reply, String>> {
        if !self.thread.as_ref()?.is_finished() {
            return None;
        }
        let thread = self.thread.take()?;
        if thread.join().is_err() {
            return Some(Err("ClothingBase worker stopped unexpectedly".into()));
        }
        Some(
            self.receiver
                .try_recv()
                .map_err(|e| e.to_string())
                .and_then(|r| r),
        )
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
