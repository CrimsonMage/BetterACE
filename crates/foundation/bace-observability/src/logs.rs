use serde::Serialize;
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const RECORD_BYTES: usize = 8192;
const RING_BYTES: usize = 4 * 1024 * 1024;
const RING_RECORDS: usize = 2000;
const FILE_BYTES: u64 = 8 * 1024 * 1024;
const FILE_COUNT: usize = 4;

#[derive(Clone, Debug, Serialize)]
pub struct LogRecord {
    pub sequence: u64,
    pub unix_millis: u64,
    pub level: String,
    pub event: String,
    pub message: String,
}
#[derive(Serialize)]
pub struct LogBatch {
    pub records: Vec<LogRecord>,
    pub gap: bool,
    pub dropped: u64,
    pub disk_degraded: bool,
}
#[derive(Default)]
struct Ring {
    records: VecDeque<LogRecord>,
    bytes: usize,
    next: u64,
}
struct Shared {
    ring: Mutex<Ring>,
    dropped: AtomicU64,
    disk_degraded: AtomicBool,
}

/// Bounded adapter diagnostics. Producers never wait for disk or queue space.
/// Callers MUST supply sanitized messages, never credentials or raw packet/SQL errors.
pub struct LogStore {
    send: Option<mpsc::SyncSender<LogRecord>>,
    shared: Arc<Shared>,
    thread: Option<thread::JoinHandle<()>>,
}
impl LogStore {
    pub fn open(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let sink = Sink::open(directory)?;
        let shared = Arc::new(Shared {
            ring: Mutex::new(Ring::default()),
            dropped: AtomicU64::new(0),
            disk_degraded: AtomicBool::new(false),
        });
        let (send, receive) = mpsc::sync_channel::<LogRecord>(256);
        let worker_shared = shared.clone();
        let thread = thread::Builder::new()
            .name("bace-diagnostics".into())
            .spawn(move || {
                let mut sink = sink;
                while let Ok(mut record) = receive.recv() {
                    {
                        let mut ring = worker_shared.ring.lock().unwrap_or_else(|e| e.into_inner());
                        ring.next = ring.next.saturating_add(1);
                        record.sequence = ring.next;
                        ring.bytes += size(&record);
                        ring.records.push_back(record.clone());
                        while ring.bytes > RING_BYTES || ring.records.len() > RING_RECORDS {
                            if let Some(old) = ring.records.pop_front() {
                                ring.bytes -= size(&old);
                            }
                        }
                    }
                    worker_shared
                        .disk_degraded
                        .store(sink.write(&record).is_err(), Ordering::Relaxed);
                }
            })?;
        Ok(Self {
            send: Some(send),
            shared,
            thread: Some(thread),
        })
    }
    pub fn record(&self, level: &str, event: &str, message: &str) {
        let record = LogRecord {
            sequence: 0,
            unix_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            level: bounded(level, 16),
            event: bounded(event, 128),
            // Reserve JSON framing and worst-case escaping while bounding the
            // on-disk/SSE record, not only the unescaped source message.
            message: bounded(message, (RECORD_BYTES - 1024) / 2),
        };
        if self
            .send
            .as_ref()
            .is_none_or(|send| send.try_send(record).is_err())
        {
            self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn recent(&self, after: u64) -> LogBatch {
        let ring = self.shared.ring.lock().unwrap_or_else(|e| e.into_inner());
        LogBatch {
            gap: ring
                .records
                .front()
                .is_some_and(|first| after != 0 && after.saturating_add(1) < first.sequence),
            records: ring
                .records
                .iter()
                .filter(|record| record.sequence > after)
                .take(128)
                .cloned()
                .collect(),
            dropped: self.shared.dropped.load(Ordering::Relaxed),
            disk_degraded: self.shared.disk_degraded.load(Ordering::Relaxed),
        }
    }
}
impl Drop for LogStore {
    fn drop(&mut self) {
        self.send.take();
        // Never block process shutdown on a failed/slow filesystem. The bounded
        // diagnostic worker drains independently; diagnostic logs are not saves.
        self.thread.take();
    }
}
fn bounded(value: &str, limit: usize) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .scan(0, |bytes, c| {
            *bytes += c.len_utf8();
            (*bytes <= limit).then_some(c)
        })
        .collect()
}
fn size(record: &LogRecord) -> usize {
    record.level.len() + record.event.len() + record.message.len() + 64
}

struct Sink {
    directory: PathBuf,
    file: Option<File>,
    bytes: u64,
}
impl Sink {
    fn open(directory: &Path) -> io::Result<Self> {
        let file = File::options()
            .create(true)
            .append(true)
            .open(directory.join("host.0.log"))?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            directory: directory.to_owned(),
            file: Some(file),
            bytes,
        })
    }
    fn write(&mut self, record: &LogRecord) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(record)?;
        bytes.push(b'\n');
        if self.bytes.saturating_add(bytes.len() as u64) > FILE_BYTES {
            self.rotate()?;
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("log rotation incomplete"))?
            .write_all(&bytes)?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }
    fn rotate(&mut self) -> io::Result<()> {
        // Close before rename; Windows need not permit renaming open files.
        self.file.take();
        let oldest = self.directory.join(format!("host.{}.log", FILE_COUNT - 1));
        if oldest.exists() {
            fs::remove_file(oldest)?;
        }
        for index in (0..FILE_COUNT - 1).rev() {
            let from = self.directory.join(format!("host.{index}.log"));
            if from.exists() {
                fs::rename(from, self.directory.join(format!("host.{}.log", index + 1)))?;
            }
        }
        self.file = Some(
            File::options()
                .create(true)
                .append(true)
                .open(self.directory.join("host.0.log"))?,
        );
        self.bytes = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
