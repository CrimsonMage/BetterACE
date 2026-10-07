//! Host dashboard owns process lifecycle; the game child owns world/save state.
use crate::control::{self, ChildMetadata, ChildReply, ChildRequest};
use bace_admin::{ControlAction, ControlRequest, HostConsole, HostStatus};
use bace_config::HostConfig;
use bace_observability::LogStore;
use std::{io, path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    process::{Child, Command},
    sync::{mpsc, watch},
};

pub async fn run_host(config: HostConfig) -> Result<(), Box<dyn std::error::Error>> {
    config.validate()?;
    bace_admin::ensure_private_directory(&config.state_directory)?;
    let _lock = control::lock(&config.state_directory.join("supervisor.lock"))?;
    let password = bace_admin::load_operator(&config.operator_file())?;
    let logs = Arc::new(LogStore::open(&config.log_directory)?);
    let listener = TcpListener::bind(config.bind_address).await?;
    let (status, receive_status) = watch::channel(HostStatus::default());
    let (control_send, mut requests) = mpsc::channel::<ControlRequest>(4);
    let console = HostConsole::new(
        config.clone(),
        password,
        logs.clone(),
        receive_status,
        control_send,
    )?;
    let (stop_web, mut web_stop) = watch::channel(false);
    let web = tokio::spawn(bace_admin::serve_console(listener, console, async move {
        let _ = web_stop.wait_for(|stop| *stop).await;
    }));
    println!(
        "BACE host console: http://{} (host authentication required)",
        config.bind_address
    );
    logs.record(
        "info",
        "host.started",
        "Host dashboard started. Stock-client game serving remains unsupported.",
    );
    let mut child = None;
    let mut metadata = match attach_or_start(&config, &mut child).await {
        Ok(metadata) => {
            let active = match call(&metadata, &ChildRequest::Status, Duration::from_secs(3)).await
            {
                Ok(ChildReply::Status { operation_id, .. }) => operation_id,
                _ => None,
            };
            publish_status(
                &status,
                &metadata,
                if active.is_some() {
                    "blocked"
                } else {
                    "running"
                },
                active,
                "Foundation child connected; game serving unsupported. Any existing drain must be retried.",
            );
            Some(metadata)
        }
        Err(_) => {
            status.send_modify(|state| {
                state.phase = "failed".into();
                state.detail =
                    "Child startup failed; inspect configuration and retry restart.".into();
            });
            logs.record("error", "child.start_failed", "Child startup failed.");
            None
        }
    };
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let mut next_operation = 1_u64;
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal?;
                let operation = status.borrow().operation_id.unwrap_or(next_operation);
                let stopped = if let Some(meta) = &metadata { drain_and_stop(&config, meta, operation, &status, &logs).await.is_ok() } else { child.is_none() && control::lock(&config.state_directory.join("child.lock")).is_ok() };
                if stopped { break; }
                logs.record("warn", "host.shutdown_blocked", "Shutdown blocked by incomplete drain. Dashboard remains available; retry the operation.");
            }
            request = requests.recv() => {
                let Some(request) = request else { break; };
                if metadata.is_none() && child.is_some() { let _=request.reply.send(Err("Child recovery pending; no replacement will be started over it.".into())); continue; }
                let phase = status.borrow().phase.clone();
                let active = status.borrow().operation_id;
                let operation = match request.action {
                    ControlAction::Retry if phase == "blocked" => active.unwrap_or(next_operation),
                    ControlAction::Retry => { let _=request.reply.send(Err("No blocked operation exists.".into())); continue; }
                    ControlAction::Restart if phase == "blocked" => { let _=request.reply.send(Err("Retry the existing blocked operation.".into())); continue; }
                    ControlAction::Restart => next_operation,
                };
                next_operation = operation.saturating_add(1);
                let _ = request.reply.send(Ok(operation));
                if let Some(meta) = &metadata
                    && drain_and_stop(&config, meta, operation, &status, &logs).await.is_err() { continue; }
                if let Some(mut previous) = child.take() { let _ = previous.wait().await; }
                metadata = None;
                status.send_modify(|state| { state.phase="starting".into();state.game_ready=false; });
                match attach_or_start(&config, &mut child).await {
                    Ok(meta) => { publish_status(&status, &meta, "running", Some(operation), "Foundation worker restarted; game serving remains unsupported."); metadata=Some(meta);logs.record("info", "child.restarted", "Foundation child restarted. No playable game is running."); }
                    Err(_) => { status.send_modify(|state| {state.phase="failed".into();state.detail="Child startup failed.".into();});logs.record("error", "child.start_failed", "Child startup failed."); }
                }
            }
            _ = poll.tick() => {
                if let Some(handle) = &mut child
                    && handle.try_wait()?.is_some() {
                        child=None;metadata=None;
                        status.send_modify(|state| {state.phase="failed".into();state.game_ready=false;state.child_pid=None;state.detail="Child exited unexpectedly. No automatic restart was attempted.".into();});
                        logs.record("error", "child.exited", "Child exited unexpectedly.");
                }
                if metadata.is_none() && child.is_some()
                    && let Ok(meta)=control::load_metadata(&config.state_directory.join("child.json"))
                    && let Ok(ChildReply::Status{operation_id,..})=call(&meta,&ChildRequest::Status,Duration::from_secs(3)).await {
                            publish_status(&status,&meta,if operation_id.is_some(){"blocked"}else{"running"},operation_id,"Recovered child control; game serving unsupported.");metadata=Some(meta);
                }
                let blocked = status.borrow().phase == "blocked";
                if !blocked
                    && let Some(meta) = &metadata
                    && let Ok(ChildReply::Status{game_ready,detail,..}) = call(meta,&ChildRequest::Status,Duration::from_secs(3)).await {
                            status.send_modify(|state| {state.game_ready=game_ready;state.detail=detail;});
                }
            }
        }
    }
    if let Some(mut child) = child {
        let _ = child.wait().await;
    }
    let _ = stop_web.send(true);
    // SSE connections may remain open; bounded teardown only affects HTTP, never saves.
    if let Ok(result) = tokio::time::timeout(Duration::from_secs(2), web).await {
        result??;
    }
    Ok(())
}
fn publish_status(
    status: &watch::Sender<HostStatus>,
    metadata: &ChildMetadata,
    phase: &str,
    operation: Option<u64>,
    detail: &str,
) {
    status.send_replace(HostStatus {
        phase: phase.into(),
        generation: metadata.generation,
        child_pid: Some(metadata.pid),
        game_ready: false,
        detail: detail.into(),
        operation_id: operation,
    });
}
async fn attach_or_start(
    config: &HostConfig,
    child: &mut Option<Child>,
) -> io::Result<ChildMetadata> {
    let path = config.state_directory.join("child.json");
    if let Ok(metadata) = control::load_metadata(&path)
        && matches!(
            call(&metadata, &ChildRequest::Status, Duration::from_secs(3)).await,
            Ok(ChildReply::Status { .. })
        )
    {
        return Ok(metadata);
    }
    // A live but unreachable child keeps this lock. Never launch over it.
    drop(control::lock(&config.state_directory.join("child.lock"))?);
    let random = control::random_bytes()?;
    let generation = u64::from_le_bytes(random[..8].try_into().map_err(io::Error::other)?);
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("__host-child")
        .arg("--state-directory")
        .arg(&config.state_directory)
        .arg("--generation")
        .arg(generation.to_string())
        .arg("--drain-timeout")
        .arg(config.drain_timeout_seconds.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    *child = Some(command.spawn()?);
    tokio::time::timeout(Duration::from_secs(config.startup_timeout_seconds), async {
        loop {
            if child
                .as_mut()
                .is_some_and(|handle| matches!(handle.try_wait(), Ok(Some(_))))
            {
                return Err(io::Error::other("child exited during startup"));
            }
            if let Ok(metadata) = control::load_metadata(&path)
                && metadata.generation == generation
                && call(&metadata, &ChildRequest::Status, Duration::from_secs(3))
                    .await
                    .is_ok()
            {
                return Ok(metadata);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("child startup timed out; retained for recovery"))?
}
async fn call(
    metadata: &ChildMetadata,
    request: &ChildRequest,
    timeout: Duration,
) -> io::Result<ChildReply> {
    tokio::time::timeout(timeout, async {
        let mut stream = control::connect(metadata).await?;
        control::write_frame(&mut stream, request).await?;
        control::read_frame(&mut stream).await
    })
    .await
    .map_err(|_| io::Error::other("child operation timed out"))?
}
async fn drain_and_stop(
    config: &HostConfig,
    metadata: &ChildMetadata,
    operation: u64,
    status: &watch::Sender<HostStatus>,
    logs: &LogStore,
) -> io::Result<()> {
    publish_status(
        status,
        metadata,
        "draining",
        Some(operation),
        "Waiting for durable drain; the child retains authoritative state.",
    );
    let result = async {
        match call(
            metadata,
            &ChildRequest::Drain {
                operation_id: operation,
            },
            Duration::from_secs(config.drain_timeout_seconds + 5),
        )
        .await?
        {
            ChildReply::Drained { operation_id } if operation_id == operation => {}
            _ => return Err(io::Error::other("durable drain is incomplete")),
        }
        publish_status(
            status,
            metadata,
            "stopping",
            Some(operation),
            "Durable drain complete; waiting for child exit.",
        );
        match call(
            metadata,
            &ChildRequest::Exit {
                operation_id: operation,
            },
            Duration::from_secs(5),
        )
        .await?
        {
            ChildReply::Exiting { operation_id } if operation_id == operation => {}
            _ => return Err(io::Error::other("child refused exit")),
        }
        await_unlock(&config.state_directory.join("child.lock")).await
    }
    .await;
    if result.is_err() {
        publish_status(
            status,
            metadata,
            "blocked",
            Some(operation),
            "Restart/shutdown blocked. Child state is retained; retry the same operation. No force termination is available.",
        );
        logs.record(
            "warn",
            "child.drain_blocked",
            "Durable drain or child exit is incomplete; state retained.",
        );
    }
    result
}
async fn await_unlock(path: &Path) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(lock) = control::lock(path) {
                drop(lock);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| io::Error::other("child did not exit; no replacement launched"))?
}
