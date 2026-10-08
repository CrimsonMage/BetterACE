use crate::document::{self, Document};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

pub(crate) enum Request {
    ScriptOpen(PathBuf),
    ScriptSave(PathBuf, String),
    Export {
        text: String,
        parent: PathBuf,
        format: crate::legacy_bundle::Format,
    },
    Open(PathBuf),
    Save {
        path: PathBuf,
        text: String,
        expected: Option<[u8; 32]>,
    },
}
pub(crate) enum Reply {
    ScriptOpened(String),
    ScriptSaved,
    Exported { path: PathBuf, notes: String },
    Opened(Document),
    Saved { path: PathBuf, text: String },
}
pub(crate) struct Job {
    receiver: Receiver<Result<Reply, String>>,
    thread: Option<JoinHandle<()>>,
}
impl Job {
    pub fn start(request: Request, ctx: eframe::egui::Context) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("content-editor-io".into())
            .spawn(move || {
                let result = match request {
                    Request::ScriptOpen(path) => document::read_bounded(&path).and_then(|text| {
                        if text.len() > 1024 * 1024 {
                            Err("Script exceeds 1 MiB".into())
                        } else {
                            Ok(Reply::ScriptOpened(text))
                        }
                    }),
                    Request::ScriptSave(path, text) => {
                        document::write(&path, &text, None).map(|()| Reply::ScriptSaved)
                    }
                    Request::Export {
                        text,
                        parent,
                        format,
                    } => crate::legacy_bundle::export(&text, &parent, format)
                        .map(|(path, notes)| Reply::Exported { path, notes }),
                    Request::Open(path) => document::open(&path).map(Reply::Opened),
                    Request::Save {
                        path,
                        text,
                        expected,
                    } => document::write(&path, &text, expected)
                        .map(|()| Reply::Saved { path, text }),
                };
                let _ = sender.send(result);
                ctx.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            receiver,
            thread: Some(thread),
        })
    }
    pub fn poll(&mut self) -> Option<Result<Reply, String>> {
        if !self.thread.as_ref().is_none_or(|t| t.is_finished()) {
            return None;
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            return Some(Err(
                "Editor worker stopped unexpectedly. Edits remain in the window.".into(),
            ));
        }
        Some(
            self.receiver
                .try_recv()
                .unwrap_or_else(|_| Err("Editor worker returned no result.".into())),
        )
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        // Exactly one reply fits without receiver progress. Owned file writes
        // finish before exiting; never detach a write with uncertain outcome.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
