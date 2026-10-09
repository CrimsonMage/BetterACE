//! Dedicated bounded network I/O owner. Application services consume events and
//! return authenticated/lifecycle decisions; this does not create a game world.
mod clock;
mod engine;
pub use clock::PortalClock;
mod types;

use bace_session::SessionKey;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
};
pub use types::{
    NetworkCommand, NetworkEvent, NetworkStopReason, NetworkThreadConfig, NetworkThreadError,
};

pub struct NetworkThread {
    commands: SyncSender<NetworkCommand>,
    events: Receiver<NetworkEvent>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<(), NetworkThreadError>>>,
    pub client_address: std::net::SocketAddr,
    pub server_address: std::net::SocketAddr,
    max_message_bytes: usize,
    max_batch_bytes: usize,
}
impl NetworkThread {
    /// Maximum one encoded outbound application message.
    pub fn maximum_message_bytes(&self) -> usize {
        self.max_message_bytes
    }

    pub fn spawn(config: NetworkThreadConfig) -> Result<Self, NetworkThreadError> {
        config.validate()?;
        let (commands, receive_commands) = mpsc::sync_channel(config.command_capacity);
        let (send_events, events) = mpsc::sync_channel(config.event_capacity);
        let (ready, start) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let max_message_bytes = config.peer.max_outgoing_message_bytes;
        let max_batch_bytes = config.peer.max_outgoing_bytes;
        let worker = thread::Builder::new()
            .name("bace-network".into())
            .spawn(move || {
                let engine =
                    engine::Engine::new(config, receive_commands, send_events, stop_worker);
                match engine {
                    Ok(mut engine) => {
                        if ready.send(Ok(engine.addresses())).is_err() {
                            return Ok(());
                        }
                        engine.run()
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error.to_string()));
                        Err(error)
                    }
                }
            })?;
        match start.recv() {
            Ok(Ok((client_address, server_address))) => Ok(Self {
                commands,
                events,
                stop,
                worker: Some(worker),
                client_address,
                server_address,
                max_message_bytes,
                max_batch_bytes,
            }),
            result => {
                let _ = worker.join();
                Err(NetworkThreadError::Startup(match result {
                    Ok(Err(e)) => e,
                    _ => "network worker exited before startup".into(),
                }))
            }
        }
    }
    /// Nonblocking admission; the caller retains a rejected command's payload.
    pub fn try_send(&self, command: NetworkCommand) -> Result<(), TrySendError<NetworkCommand>> {
        if let NetworkCommand::SendReliableBatch {
            correlation,
            messages,
            ..
        } = &command
            && (*correlation == 0
                || messages.is_empty()
                || messages.len() > 256
                || messages.iter().any(|(queue, bytes)| {
                    *queue >= 12 || !(4..=self.max_message_bytes).contains(&bytes.len())
                })
                || messages
                    .iter()
                    .try_fold(0usize, |n, (_, bytes)| n.checked_add(bytes.len()))
                    .is_none_or(|n| n > self.max_batch_bytes))
        {
            return Err(TrySendError::Full(command));
        }
        if let NetworkCommand::Send { bytes, .. } = &command
            && (bytes.len() < 4 || bytes.len() > self.max_message_bytes)
        {
            return Err(TrySendError::Full(command));
        }
        if let NetworkCommand::SendBatch {
            queue, messages, ..
        } = &command
        {
            let valid = !messages.is_empty()
                && messages.len() <= 256
                && *queue < 12
                && messages
                    .iter()
                    .all(|bytes| (4..=self.max_message_bytes).contains(&bytes.len()))
                && messages
                    .iter()
                    .try_fold(0usize, |total, bytes| total.checked_add(bytes.len()))
                    .is_some_and(|total| total <= self.max_batch_bytes);
            if !valid {
                return Err(TrySendError::Full(command));
            }
        }
        self.commands.try_send(command)
    }
    pub fn events(&self) -> &Receiver<NetworkEvent> {
        &self.events
    }
    pub fn terminate(&self, key: SessionKey) -> Result<(), TrySendError<NetworkCommand>> {
        self.try_send(NetworkCommand::Terminate { key })
    }
    /// Stop network servicing only AFTER application owners have drained. This
    /// does not acknowledge character durability or release world ownership.
    pub fn shutdown(mut self) -> Result<(), NetworkThreadError> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .ok_or(NetworkThreadError::Closed)?
            .join()
            .map_err(|_| NetworkThreadError::Panic)?
    }
}
impl Drop for NetworkThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
