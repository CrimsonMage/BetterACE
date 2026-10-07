//! Dedicated OS thread owns a current-thread async runtime and its writer pool.
use crate::saves::{
    SaveBackend, SaveHandle, SaveWorkerConfig, SaveWorkerSummary, spawn_save_worker,
};
use bace_db_postgres::PgStore;
use std::{future::Future, thread, time::Duration};
use tokio::sync::oneshot;

#[derive(Debug, thiserror::Error)]
pub enum PersistenceThreadError {
    #[error("persistence thread startup failed: {0}")]
    Startup(String),
    #[error("persistence thread completion channel closed")]
    Closed,
    #[error("persistence thread panicked")]
    Panic,
}
pub struct PersistenceThread {
    pub handle: SaveHandle,
    pub thread_id: thread::ThreadId,
    completion: oneshot::Receiver<Result<SaveWorkerSummary, PersistenceThreadError>>,
    thread: thread::JoinHandle<()>,
}
impl PersistenceThread {
    /// Pool construction occurs inside the new thread, never on the caller's executor.
    pub async fn postgres(
        url: String,
        config: SaveWorkerConfig,
    ) -> Result<Self, PersistenceThreadError> {
        let timeout = config.operation_timeout;
        Self::spawn(config, move || async move {
            PgStore::connect_writer(&url, timeout)
                .await
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// Factory supports fault injection; it executes on the persistence OS thread.
    pub async fn spawn<B, F, Fut>(
        config: SaveWorkerConfig,
        factory: F,
    ) -> Result<Self, PersistenceThreadError>
    where
        B: SaveBackend,
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<B, String>>,
    {
        let (ready_tx, ready_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let thread = thread::Builder::new()
            .name("bace-persistence".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = ready_tx.send(Err(PersistenceThreadError::Startup(e.to_string())));
                        return;
                    }
                };
                let result = runtime.block_on(async move {
                    let backend = match factory().await {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = ready_tx.send(Err(PersistenceThreadError::Startup(e)));
                            return Err(PersistenceThreadError::Closed);
                        }
                    };
                    let worker = match spawn_save_worker(backend, config) {
                        Ok(w) => w,
                        Err(e) => {
                            let _ =
                                ready_tx.send(Err(PersistenceThreadError::Startup(e.to_string())));
                            return Err(PersistenceThreadError::Closed);
                        }
                    };
                    let _ = ready_tx.send(Ok(worker.handle));
                    worker.task.await.map_err(|_| PersistenceThreadError::Panic)
                });
                runtime.shutdown_timeout(Duration::from_secs(5));
                let _ = done_tx.send(result);
            })
            .map_err(|e| PersistenceThreadError::Startup(e.to_string()))?;
        let thread_id = thread.thread().id();
        let handle = match ready_rx.await {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                let _ = thread.join();
                return Err(PersistenceThreadError::Closed);
            }
        };
        Ok(Self {
            handle,
            thread_id,
            completion: done_rx,
            thread,
        })
    }
    /// Closes all handle clones to new admission. Healthy admitted work drains before join.
    /// Storage failures return tickets to owners; this cannot promise outage durability.
    pub async fn drain(self) -> Result<SaveWorkerSummary, PersistenceThreadError> {
        self.handle.close();
        drop(self.handle);
        let result = self
            .completion
            .await
            .map_err(|_| PersistenceThreadError::Closed)?;
        self.thread
            .join()
            .map_err(|_| PersistenceThreadError::Panic)?;
        result
    }
}
