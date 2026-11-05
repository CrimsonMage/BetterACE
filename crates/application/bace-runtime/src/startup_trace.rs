//! Startup timing visible to direct serve and the authenticated host log.
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
pub(crate) struct StartupTrace {
    inner: Arc<Inner>,
}

struct Inner {
    started: Instant,
    current: Mutex<&'static str>,
    log: Arc<bace_observability::LogStore>,
}

pub(crate) struct StartupStep {
    trace: StartupTrace,
    name: &'static str,
    started: Instant,
}

impl StartupTrace {
    pub(crate) fn new(log: Arc<bace_observability::LogStore>) -> Self {
        Self {
            inner: Arc::new(Inner {
                started: Instant::now(),
                current: Mutex::new("starting"),
                log,
            }),
        }
    }

    pub(crate) fn step(&self, name: &'static str) -> StartupStep {
        *self.inner.current.lock().unwrap_or_else(|e| e.into_inner()) = name;
        self.emit("start", name, None);
        StartupStep {
            trace: self.clone(),
            name,
            started: Instant::now(),
        }
    }

    pub(crate) fn current(&self) -> &'static str {
        *self.inner.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn elapsed_seconds(&self) -> f64 {
        self.inner.started.elapsed().as_secs_f64()
    }

    fn emit(&self, action: &str, name: &str, stage_seconds: Option<f64>) {
        let message = if let Some(seconds) = stage_seconds {
            format!(
                "startup +{:.3}s: {name} ended after {seconds:.3}s",
                self.elapsed_seconds()
            )
        } else {
            format!("startup +{:.3}s: {name} {action}", self.elapsed_seconds())
        };
        eprintln!("{message}");
        self.inner.log.record("info", "game.startup", &message);
    }
}

/// Best-effort process totals sampled once when game sockets are ready. The
/// elapsed startup clock remains independent of optional OS counters.
pub(crate) fn resource_summary() -> String {
    #[cfg(target_os = "linux")]
    {
        let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
            return "resource counters unavailable".into();
        };
        let field = |name: &str| -> Option<u64> {
            status.lines().find_map(|line| {
                line.strip_prefix(name)?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
        };
        let mut parts = Vec::new();
        if let Some(rss) = field("VmRSS:") {
            parts.push(format!("RSS {:.1} MiB", rss as f64 / 1024.0));
        }
        if let Some(peak) = field("VmHWM:") {
            parts.push(format!("peak RSS {:.1} MiB", peak as f64 / 1024.0));
        }
        if let Some(cpu) = process_cpu_seconds() {
            parts.push(format!("CPU {cpu:.3}s"));
        }
        if let Some(threads) = field("Threads:") {
            parts.push(format!("{threads} threads"));
        }
        if parts.is_empty() {
            "resource counters unavailable".into()
        } else {
            parts.join(", ")
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        "resource counters unavailable on this platform".into()
    }
}

#[cfg(target_os = "linux")]
fn process_cpu_seconds() -> Option<f64> {
    // /proc/self/stat fields after the final ')' start with field 3 (state).
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields: Vec<_> = stat
        .get(stat.rfind(')')? + 1..)?
        .split_whitespace()
        .collect();
    let user: u64 = fields.get(11)?.parse().ok()?;
    let system: u64 = fields.get(12)?.parse().ok()?;
    let output = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let ticks_per_second: u64 = std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if ticks_per_second == 0 {
        return None;
    }
    Some((user.checked_add(system)? as f64) / ticks_per_second as f64)
}

impl Drop for StartupStep {
    fn drop(&mut self) {
        self.trace.emit(
            "ended",
            self.name,
            Some(self.started.elapsed().as_secs_f64()),
        );
    }
}
