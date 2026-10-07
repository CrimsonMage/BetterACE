use crate::sql_process::wait_bounded;
use std::{
    fs::File,
    process::{Command, Stdio},
    time::Duration,
};

#[cfg(unix)]
fn sleeping_child(directory: &std::path::Path) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    Command::new("/bin/sh")
        .args(["-c", "sleep 5"])
        .process_group(0)
        .stdout(Stdio::from(File::create(directory.join("stdout")).unwrap()))
        .stderr(Stdio::from(File::create(directory.join("stderr")).unwrap()))
        .spawn()
        .unwrap()
}

#[test]
#[cfg(unix)]
fn subprocess_deadline_reaps_child_and_bootstrap_group() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = sleeping_child(directory.path());
    let result = wait_bounded(
        &mut child,
        &directory.path().join("stdout"),
        &directory.path().join("stderr"),
        1024,
        Duration::from_millis(25),
    );
    assert!(result.unwrap_err().to_string().contains("deadline"));
    assert!(child.try_wait().unwrap().is_some());
}

#[test]
#[cfg(unix)]
fn subprocess_output_limit_terminates_before_accepting_result() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = sleeping_child(directory.path());
    std::fs::write(directory.path().join("stdout"), [0_u8; 64]).unwrap();
    let result = wait_bounded(
        &mut child,
        &directory.path().join("stdout"),
        &directory.path().join("stderr"),
        32,
        Duration::from_secs(1),
    );
    assert!(result.unwrap_err().to_string().contains("output limit"));
    assert!(child.try_wait().unwrap().is_some());
}
