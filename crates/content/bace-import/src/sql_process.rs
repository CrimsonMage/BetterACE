use crate::SqlStagingError;
use std::{
    path::Path,
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant},
};

/// File-backed stdout/stderr avoid pipe deadlocks. Poll limits while the process
/// runs, and always kill/reap our own child on timeout or monitoring failure.
pub(crate) fn wait_bounded(
    child: &mut Child,
    stdout: &Path,
    stderr: &Path,
    max_stdout: u64,
    timeout: Duration,
) -> Result<ExitStatus, SqlStagingError> {
    let result = (|| {
        let deadline = Instant::now() + timeout;
        loop {
            let out = std::fs::metadata(stdout)
                .map_err(|e| SqlStagingError::Extraction(e.to_string()))?
                .len();
            let err = std::fs::metadata(stderr)
                .map_err(|e| SqlStagingError::Extraction(e.to_string()))?
                .len();
            if out > max_stdout || err > 1024 * 1024 {
                return Err(SqlStagingError::Extraction(
                    "MariaDB subprocess output limit exceeded".into(),
                ));
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|e| SqlStagingError::Extraction(e.to_string()))?
            {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(SqlStagingError::Extraction(
                    "MariaDB subprocess deadline exceeded".into(),
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    })();
    if result.is_err() {
        terminate(child);
    }
    result
}

/// Initializer scripts can own a bootstrap-server child. Terminate the process
/// group created at spawn, then reap the direct child; never signal other groups.
pub(crate) fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
