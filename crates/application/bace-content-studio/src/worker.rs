use crate::{ConversionOptions, ConversionSummary, convert_file};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::thread::JoinHandle;

pub(crate) const MAX_FILES: usize = 128;

pub(crate) struct FileResult {
    pub source: PathBuf,
    pub result: Result<ConversionSummary, String>,
}

pub(crate) struct Worker {
    receiver: Option<Receiver<FileResult>>,
    thread: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}

impl Worker {
    pub fn start(
        files: Vec<PathBuf>,
        options: ConversionOptions,
        repaint: eframe::egui::Context,
    ) -> Result<Self, String> {
        if files.is_empty() || files.len() > MAX_FILES {
            return Err(format!("Select between 1 and {MAX_FILES} files."));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&cancel);
        let thread = std::thread::Builder::new()
            .name("content-conversion".into())
            .spawn(move || {
                for source in files {
                    if stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = convert_file(&source, &options, &stopped);
                    if sender.send(FileResult { source, result }).is_err() {
                        break;
                    }
                    repaint.request_repaint();
                }
                repaint.request_repaint();
            })
            .map_err(|e| format!("Could not start conversion: {e}"))?;
        Ok(Self {
            receiver: Some(receiver),
            thread: Some(thread),
            cancel,
        })
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn receive(&self) -> Option<FileResult> {
        self.receiver.as_ref()?.try_recv().ok()
    }

    pub fn finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }

    pub fn finish(&mut self) -> Result<(), String> {
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| "The conversion worker stopped unexpectedly; inspect the output folder before retrying.".to_string())?;
        }
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        // Disconnect before joining so a full result channel cannot deadlock
        // shutdown. Owned MariaDB cleanup must complete before process exit.
        self.receiver.take();
        let _ = self.finish();
    }
}
