use crate::control::{self, ChildMetadata, ChildReply, ChildRequest};
use std::{future::Future, io, path::Path, time::Duration};
use tokio::net::TcpListener;

/// The world/save owner MUST retain dirty state after an error or cancellation.
/// Drained means every admitted persistent mutation is durably acknowledged.
/// The caller never force-kills the backend on a drain failure.
pub trait HostBackend {
    fn status(&self) -> (bool, String);
    /// Stop admission after supervisor lease loss; preserve the world for recovery.
    fn control_lost(&mut self);
    fn drain(&mut self, operation_id: u64) -> impl Future<Output = Result<(), String>> + Send;
}
pub struct FoundationBackend;
impl HostBackend for FoundationBackend {
    fn control_lost(&mut self) {}
    fn status(&self) -> (bool, String) {
        (false, "Foundation worker only. Game serving unsupported; no game socket, world or database was opened.".into())
    }
    async fn drain(&mut self, _operation_id: u64) -> Result<(), String> {
        // This backend never owns a world or saves, so there is nothing to lose.
        Ok(())
    }
}
pub async fn run_child<B: HostBackend>(
    directory: &Path,
    generation: u64,
    drain_timeout: Duration,
    mut backend: B,
) -> io::Result<()> {
    bace_admin::ensure_private_directory(directory).map_err(io::Error::other)?;
    let _lock = control::lock(&directory.join("child.lock"))?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let metadata = ChildMetadata {
        version: 1,
        port: listener.local_addr()?.port(),
        generation,
        pid: std::process::id(),
        capability: control::random_bytes()?,
    };
    control::store_metadata(&directory.join("child.json"), &metadata)?;
    let mut operation = None;
    let mut drained = false;
    let mut last_control = tokio::time::Instant::now();
    let mut control_lost = false;
    loop {
        let accepted = tokio::time::timeout(Duration::from_secs(1), listener.accept()).await;
        if last_control.elapsed() >= Duration::from_secs(10) && !control_lost {
            backend.control_lost();
            control_lost = true;
        }
        let Ok(accepted) = accepted else {
            continue;
        };
        let (mut stream, peer) = accepted?;
        if !peer.ip().is_loopback() {
            continue;
        }
        if !matches!(
            tokio::time::timeout(
                Duration::from_secs(2),
                control::server_auth(&mut stream, &metadata)
            )
            .await,
            Ok(Ok(()))
        ) {
            continue;
        }
        last_control = tokio::time::Instant::now();
        control_lost = false;
        loop {
            let request = match tokio::time::timeout(
                Duration::from_secs(10),
                control::read_frame::<ChildRequest>(&mut stream),
            )
            .await
            {
                Ok(Ok(request)) => request,
                _ => break, // Retain backend and lock for authenticated reconnection.
            };
            let reply = match request {
                ChildRequest::Status => {
                    let (game_ready, detail) = backend.status();
                    ChildReply::Status {
                        game_ready,
                        detail,
                        operation_id: operation,
                    }
                }
                ChildRequest::Drain { operation_id } => {
                    if operation.is_some_and(|active| active != operation_id) {
                        ChildReply::Blocked {
                            operation_id,
                            detail: "A different drain operation already owns this child.".into(),
                        }
                    } else {
                        operation = Some(operation_id);
                        if !drained {
                            match tokio::time::timeout(drain_timeout, backend.drain(operation_id))
                                .await
                            {
                                Ok(Ok(())) => drained = true,
                                Ok(Err(_)) => {} // Backend errors may contain secrets: use a stable public message.
                                Err(_) => {}
                            }
                        }
                        if drained {
                            ChildReply::Drained { operation_id }
                        } else {
                            ChildReply::Blocked { operation_id, detail: "Durable drain is incomplete. The child retains its state; retry the same operation.".into() }
                        }
                    }
                }
                ChildRequest::Exit { operation_id } => {
                    if operation == Some(operation_id) && drained {
                        control::write_frame(&mut stream, &ChildReply::Exiting { operation_id })
                            .await?;
                        return Ok(());
                    }
                    ChildReply::Blocked {
                        operation_id,
                        detail: "Exit refused: durable drain has not completed.".into(),
                    }
                }
            };
            if control::write_frame(&mut stream, &reply).await.is_err() {
                break;
            }
        }
    }
}
