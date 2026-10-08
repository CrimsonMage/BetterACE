//! Fixed-size authentication workers and bounded admission/completion queues.
use crate::authentication::PasswordExecutor;
use bace_auth::{AccountRecord, AccountRepository, LoginError, PasswordCredentials, authenticate};
use bace_config::{AccountConfig, NetworkConfig};
use bace_persistence::{
    AccountBanChange, AccountBanOperation, AccountBanReceipt, AccountBanVerdict,
};
use bace_session::{SessionKey, validate_password_login};
use bace_types::AccountId;
use bace_wire::LoginRequest;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, mpsc};

#[derive(Debug)]
pub struct AuthenticationJob {
    pub key: SessionKey,
    pub request: LoginRequest,
}
pub trait BanRepository: AccountRepository {
    fn ban_verdict(
        &self,
        account_id: AccountId,
        now_unix_millis: i64,
    ) -> impl std::future::Future<Output = Result<AccountBanVerdict, Self::Error>> + Send;
    fn expire_ban(
        &self,
        operation: &AccountBanOperation,
    ) -> impl std::future::Future<Output = Result<AccountBanReceipt, Self::Error>> + Send;
}

impl BanRepository for bace_db_postgres::PgStore {
    async fn ban_verdict(
        &self,
        account_id: AccountId,
        now_unix_millis: i64,
    ) -> Result<AccountBanVerdict, Self::Error> {
        self.account_ban_verdict(account_id.0, now_unix_millis)
            .await
    }
    async fn expire_ban(
        &self,
        operation: &AccountBanOperation,
    ) -> Result<AccountBanReceipt, Self::Error> {
        self.apply_account_ban(operation).await
    }
}

async fn check_account_ban<R: BanRepository>(
    repository: &R,
    account_id: AccountId,
) -> Result<(), AuthenticationFailure> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AuthenticationFailure::Repository)?;
    let now = i64::try_from(now.as_millis()).map_err(|_| AuthenticationFailure::Repository)?;
    // A concurrent ban/unban can race this read. Bound retries and fail closed
    // if its revision cannot be resolved before account admission.
    for _ in 0..3 {
        let verdict = repository
            .ban_verdict(account_id, now)
            .await
            .map_err(|_| AuthenticationFailure::Repository)?;
        match verdict {
            AccountBanVerdict::Allowed => return Ok(()),
            AccountBanVerdict::Missing | AccountBanVerdict::Disabled => {
                return Err(AuthenticationFailure::Rejected);
            }
            AccountBanVerdict::Banned(record) => {
                let remaining = record.expires_unix_millis.saturating_sub(now) / 1_000;
                return Err(AuthenticationFailure::Banned {
                    seconds_remaining: u32::try_from(remaining).unwrap_or(u32::MAX),
                    reason: record.reason.unwrap_or_default(),
                });
            }
            AccountBanVerdict::Expired(record) => {
                let mut digest = Sha256::new();
                digest.update(b"BetterACE/account-ban-expiry/v1");
                digest.update(account_id.0.to_le_bytes());
                digest.update(record.account_revision.to_le_bytes());
                digest.update(record.expires_unix_millis.to_le_bytes());
                let hash = digest.finalize();
                let mut operation_id = [0; 16];
                operation_id.copy_from_slice(&hash[..16]);
                let operation = AccountBanOperation {
                    operation_id,
                    account_id: account_id.0,
                    expected_revision: record.account_revision,
                    issuer_account_id: None,
                    // The exact expiry instant is stable across retrying logins,
                    // matching the deterministic operation ID and fingerprint.
                    change: AccountBanChange::Expire {
                        now_unix_millis: record.expires_unix_millis,
                    },
                };
                // A replay uses this exact deterministic ID; an uncertain commit
                // never causes a new operation to be invented on the next login.
                repository
                    .expire_ban(&operation)
                    .await
                    .map_err(|_| AuthenticationFailure::Repository)?;
            }
        }
    }
    Err(AuthenticationFailure::Repository)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticationFailure {
    InvalidLogin,
    Rejected,
    Busy,
    Repository,
    Password,
    TimedOut,
    Banned {
        seconds_remaining: u32,
        reason: String,
    },
}
#[derive(Debug)]
pub struct AuthenticationCompletion {
    pub key: SessionKey,
    pub result: Result<AccountRecord, AuthenticationFailure>,
}

pub struct AuthenticationDrain {
    pub completions: Vec<AuthenticationCompletion>,
    pub unrecovered_sessions: Vec<SessionKey>,
    pub panicked_workers: usize,
    pub passwords_drained: bool,
}

pub struct AuthenticationPool {
    input: Option<mpsc::Sender<AuthenticationJob>>,
    results: mpsc::Receiver<AuthenticationCompletion>,
    workers: Vec<tokio::task::JoinHandle<()>>,
    passwords: PasswordExecutor,
    outstanding: StdMutex<BTreeSet<SessionKey>>,
    capacity: usize,
}
impl AuthenticationPool {
    /// Must be called inside the adapter runtime. Workers are fixed at startup;
    /// no task is created per account/character. Account SQL remains in its port.
    pub fn spawn<R: BanRepository + 'static>(
        repository: Arc<R>,
        config: &NetworkConfig,
        policy: AccountConfig,
    ) -> Result<Self, String> {
        config.validate().map_err(|e| e.to_string())?;
        let passwords =
            PasswordExecutor::new(config.authentication_workers).map_err(|e| e.to_string())?;
        let (input, receiver) = mpsc::channel::<AuthenticationJob>(config.authentication_queue);
        let receiver = Arc::new(Mutex::new(receiver));
        let (output, results) = mpsc::channel(config.authentication_queue);
        let mut workers = Vec::with_capacity(config.authentication_workers);
        for _ in 0..config.authentication_workers {
            let repository = repository.clone();
            let passwords = passwords.clone();
            let receiver = receiver.clone();
            let output = output.clone();
            let auto = policy.allow_auto_creation;
            let timeout = Duration::from_millis(config.authentication_timeout_ms);
            workers.push(tokio::spawn(async move {
                loop {
                    let job = receiver.lock().await.recv().await;
                    let Some(job) = job else {
                        break;
                    };
                    let result =
                        match validate_password_login(&job.request)
                            .ok()
                            .and_then(|login| {
                                PasswordCredentials::new(login.account, login.password.as_bytes())
                                    .ok()
                            }) {
                            None => Err(AuthenticationFailure::InvalidLogin),
                            Some(credentials) => match tokio::time::timeout(
                                timeout,
                                authenticate(repository.as_ref(), &passwords, credentials, auto),
                            )
                            .await
                            {
                                Err(_) => Err(AuthenticationFailure::TimedOut),
                                Ok(Ok(account)) => match tokio::time::timeout(
                                    timeout,
                                    check_account_ban(repository.as_ref(), account.id),
                                )
                                .await
                                {
                                    Ok(Ok(())) => Ok(account),
                                    Ok(Err(failure)) => Err(failure),
                                    Err(_) => Err(AuthenticationFailure::TimedOut),
                                },
                                Ok(Err(LoginError::Rejected)) => {
                                    Err(AuthenticationFailure::Rejected)
                                }
                                Ok(Err(LoginError::Repository(_))) => {
                                    Err(AuthenticationFailure::Repository)
                                }
                                Ok(Err(LoginError::Password(bace_auth::AuthError::Busy))) => {
                                    Err(AuthenticationFailure::Busy)
                                }
                                Ok(Err(LoginError::Password(_))) => {
                                    Err(AuthenticationFailure::Password)
                                }
                            },
                        };
                    if output
                        .send(AuthenticationCompletion {
                            key: job.key,
                            result,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        drop(output);
        Ok(Self {
            input: Some(input),
            results,
            workers,
            passwords,
            outstanding: StdMutex::new(BTreeSet::new()),
            capacity: config.authentication_queue,
        })
    }
    pub fn try_submit(
        &self,
        job: AuthenticationJob,
    ) -> Result<(), Box<mpsc::error::TrySendError<AuthenticationJob>>> {
        let credential_bytes = match &job.request.credential {
            bace_wire::LoginCredential::Password(value)
            | bace_wire::LoginCredential::GlsTicket(value) => value.len(),
            _ => 0,
        };
        let total = job
            .request
            .client_version
            .len()
            .saturating_add(job.request.account.len())
            .saturating_add(job.request.account_to_login_as.len())
            .saturating_add(credential_bytes);
        if total > bace_wire::CLIENT_DATAGRAM_LIMIT {
            return Err(Box::new(mpsc::error::TrySendError::Full(job)));
        }
        let Ok(mut outstanding) = self.outstanding.lock() else {
            return Err(Box::new(mpsc::error::TrySendError::Closed(job)));
        };
        if outstanding.len() == self.capacity || outstanding.contains(&job.key) {
            return Err(Box::new(mpsc::error::TrySendError::Full(job)));
        }
        let key = job.key;
        self.input
            .as_ref()
            .expect("active authentication pool")
            .try_send(job)?;
        outstanding.insert(key);
        Ok(())
    }
    pub async fn next(&mut self) -> Option<AuthenticationCompletion> {
        let result = self.results.recv().await?;
        if let Ok(mut outstanding) = self.outstanding.lock() {
            outstanding.remove(&result.key);
        }
        Some(result)
    }
    /// Close admission, preserve every accepted completion, and join workers.
    /// The caller must generation-check results before issuing Authenticated.
    pub async fn shutdown(mut self) -> AuthenticationDrain {
        drop(self.input.take());
        let mut completions = Vec::new();
        while let Some(result) = self.next().await {
            completions.push(result);
        }
        let mut panicked_workers = 0;
        for worker in self.workers.drain(..) {
            if worker.await.is_err() {
                panicked_workers += 1;
            }
        }
        let passwords_drained = self.passwords.wait_idle().await.is_ok();
        let unrecovered_sessions = self
            .outstanding
            .lock()
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        AuthenticationDrain {
            completions,
            unrecovered_sessions,
            panicked_workers,
            passwords_drained,
        }
    }
}
impl Drop for AuthenticationPool {
    fn drop(&mut self) {
        // Aborted async callers cannot release a running password permit; the
        // spawn_blocking closure owns it. Drop is not a graceful-drain receipt.
        for worker in &self.workers {
            worker.abort();
        }
    }
}
