//! Bounded host bridge to one adapter thread. The non-Send GameRuntime and its
//! retained startup/shutdown futures never leave that thread; physics has its
//! separate existing single simulation owner.
mod driver;
#[cfg(test)]
mod tests;
use crate::supervisor_child::HostBackend;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, watch};
#[derive(Clone, Debug)]
struct Status {
    ready: bool,
    drained: bool,
    detail: String,
}
pub struct GameBackend {
    controls: mpsc::Sender<u64>,
    status: watch::Receiver<Status>,
    lost: Arc<AtomicBool>,
    operation: Option<u64>,
}
impl GameBackend {
    pub fn start(config: bace_config::ServerConfig) -> Result<Self, String> {
        config.validate().map_err(|e| e.to_string())?;
        let (send, receive) = mpsc::channel(2);
        let (publish, status) = watch::channel(Status {
            ready: false,
            drained: false,
            detail: "Preparing accepted content and verified assets.".into(),
        });
        let lost = Arc::new(AtomicBool::new(false));
        let worker_lost = lost.clone();
        std::thread::Builder::new()
            .name("betterace-game-adapter".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        runtime.block_on(driver::run(config, receive, publish, worker_lost))
                    }
                    Err(_) => {
                        publish.send_replace(Status {
                            ready: false,
                            drained: true,
                            detail: "Game adapter runtime could not start; no world was opened."
                                .into(),
                        });
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            controls: send,
            status,
            lost,
            operation: None,
        })
    }
}
impl HostBackend for GameBackend {
    fn status(&self) -> (bool, String) {
        let state = self.status.borrow();
        (state.ready, state.detail.clone())
    }
    fn control_lost(&mut self) {
        self.lost.store(true, Ordering::Release);
    }
    async fn drain(&mut self, operation_id: u64) -> Result<(), String> {
        if self.operation.is_some_and(|old| old != operation_id) {
            return Err("another drain owns the game".into());
        }
        if self.status.borrow().drained {
            return Ok(());
        }
        if self.operation.is_none() {
            self.controls
                .send(operation_id)
                .await
                .map_err(|_| "game control owner closed")?;
            self.operation = Some(operation_id);
        }
        loop {
            if self.status.borrow().drained {
                return Ok(());
            }
            self.status
                .changed()
                .await
                .map_err(|_| "game worker exited without a durable shutdown proof")?;
        }
    }
}
impl Drop for GameBackend {
    fn drop(&mut self) {
        // A dropped control connection cannot abandon a dirty child. The OS
        // thread retains the entire owner and quiesces on the next adapter turn.
        self.lost.store(true, Ordering::Release);
    }
}
pub async fn serve(config: bace_config::ServerConfig) -> Result<(), String> {
    let mut backend = GameBackend::start(config)?;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut prior = String::new();
    loop {
        tokio::select! {
            signal=tokio::signal::ctrl_c()=>{
                signal.map_err(|e|e.to_string())?;
                eprintln!("Draining admitted characters and world saves; the process retains unresolved work.");
                backend.drain(1).await?;
                return Ok(());
            }
            _=interval.tick()=>{
                let (_,detail)=backend.status();
                if detail!=prior {eprintln!("{detail}");prior=detail;}
            }
        }
    }
}
