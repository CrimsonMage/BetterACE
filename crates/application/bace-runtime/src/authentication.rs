//! Bounded blocking password work; neither reactor nor simulation executes Argon2.
use bace_auth::{AuthError, PasswordHashRecord, PasswordService, PasswordWorker};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct PasswordExecutor {
    service: Arc<PasswordService>,
    permits: Arc<Semaphore>,
    workers: u32,
}

impl PasswordExecutor {
    pub fn new(workers: usize) -> Result<Self, AuthError> {
        Ok(Self {
            service: Arc::new(PasswordService::new(workers)?),
            permits: Arc::new(Semaphore::new(workers)),
            workers: workers as u32,
        })
    }
}

impl PasswordExecutor {
    /// After closing all admission, wait for blocking closures even if their
    /// async requests timed out. Cancellation never releases their work budget.
    pub async fn wait_idle(&self) -> Result<(), AuthError> {
        let permit = self
            .permits
            .clone()
            .acquire_many_owned(self.workers)
            .await
            .map_err(|_| AuthError::Busy)?;
        drop(permit);
        Ok(())
    }
}

impl PasswordWorker for PasswordExecutor {
    async fn hash(&self, password: Vec<u8>) -> Result<PasswordHashRecord, AuthError> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Busy)?;
        let service = self.service.clone();
        tokio::task::spawn_blocking(move || {
            // The permit stays with the work even if its caller is cancelled.
            let _permit = permit;
            service.hash(&password)
        })
        .await
        .map_err(|_| AuthError::HashFailure)?
    }
    async fn verify(&self, password: Vec<u8>, hash: PasswordHashRecord) -> Result<bool, AuthError> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Busy)?;
        let service = self.service.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            service.verify(&password, &hash)
        })
        .await
        .map_err(|_| AuthError::HashFailure)?
    }
}
