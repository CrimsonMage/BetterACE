use bace_auth::*;
use bace_config::{AccountConfig, NetworkConfig};
use bace_runtime::authentication_pool::*;
use bace_session::SessionKey;
use bace_types::AccountId;
use bace_wire::{LoginCredential, LoginRequest, NetAuthType};
use std::sync::{Arc, Mutex};
struct Accounts(Mutex<Option<AccountRecord>>);
impl AccountRepository for Accounts {
    type Error = std::io::Error;
    async fn find_by_name(&self, _: &AccountName) -> Result<Option<AccountRecord>, Self::Error> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn create(&self, new: NewAccount) -> Result<CreateAccountOutcome, Self::Error> {
        let mut account = self.0.lock().unwrap();
        if account.is_some() {
            return Ok(CreateAccountOutcome::AlreadyExists);
        }
        let record = AccountRecord {
            id: AccountId(1),
            name: new.name,
            password_hash: new.password_hash,
            access_level: AccessLevel::Player,
            disabled: false,
        };
        *account = Some(record.clone());
        Ok(CreateAccountOutcome::Created(record))
    }
}
impl BanRepository for Accounts {
    async fn ban_verdict(
        &self,
        _: AccountId,
        _: i64,
    ) -> Result<bace_persistence::AccountBanVerdict, Self::Error> {
        Ok(bace_persistence::AccountBanVerdict::Allowed)
    }
    async fn expire_ban(
        &self,
        _: &bace_persistence::AccountBanOperation,
    ) -> Result<bace_persistence::AccountBanReceipt, Self::Error> {
        unreachable!("an unbanned test account has no expiry")
    }
}
fn job(generation: u64, password: &str) -> AuthenticationJob {
    AuthenticationJob {
        key: SessionKey { id: 0, generation },
        request: LoginRequest {
            client_version: "1802".into(),
            declared_length: 0,
            net_auth_type: NetAuthType::AccountPassword,
            auth_flags: 0,
            timestamp: 0,
            account: "NativePlayer".into(),
            account_to_login_as: "Admin".into(),
            credential: LoginCredential::Password(password.into()),
            trailing_bytes: 0,
        },
    }
}
#[tokio::test]
async fn default_creation_and_existing_password_verification_use_bounded_workers() {
    let accounts = Arc::new(Accounts(Mutex::new(None)));
    let mut pool = AuthenticationPool::spawn(
        accounts.clone(),
        &NetworkConfig::default(),
        AccountConfig::default(),
    )
    .unwrap();
    pool.try_submit(job(1, "synthetic-password")).unwrap();
    let first = pool.next().await.unwrap();
    assert_eq!(first.key.generation, 1);
    assert_eq!(first.result.unwrap().access_level, AccessLevel::Player);
    pool.try_submit(job(2, "wrong-password")).unwrap();
    let drain = pool.shutdown().await;
    assert_eq!(drain.panicked_workers, 0);
    assert!(drain.passwords_drained);
    assert!(drain.unrecovered_sessions.is_empty());
    let completions = drain.completions;
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].key.generation, 2);
    assert!(matches!(
        completions[0].result,
        Err(AuthenticationFailure::Rejected)
    ));
    assert_eq!(
        accounts.0.lock().unwrap().as_ref().unwrap().name.as_str(),
        "nativeplayer"
    );
}
#[tokio::test]
async fn empty_worker_shutdown_does_not_wait_for_a_new_job() {
    let pool = AuthenticationPool::spawn(
        Arc::new(Accounts(Mutex::new(None))),
        &NetworkConfig::default(),
        AccountConfig::default(),
    )
    .unwrap();
    tokio::task::yield_now().await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), pool.shutdown())
            .await
            .unwrap()
            .completions
            .is_empty()
    );
}

struct PanickingAccounts {
    accounts: Accounts,
    finds: std::sync::atomic::AtomicUsize,
}
impl AccountRepository for PanickingAccounts {
    type Error = std::io::Error;
    async fn find_by_name(&self, name: &AccountName) -> Result<Option<AccountRecord>, Self::Error> {
        assert_eq!(
            self.finds.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            0,
            "injected worker panic"
        );
        self.accounts.find_by_name(name).await
    }
    async fn create(&self, new: NewAccount) -> Result<CreateAccountOutcome, Self::Error> {
        self.accounts.create(new).await
    }
}
impl BanRepository for PanickingAccounts {
    async fn ban_verdict(
        &self,
        account_id: AccountId,
        now_unix_millis: i64,
    ) -> Result<bace_persistence::AccountBanVerdict, Self::Error> {
        self.accounts.ban_verdict(account_id, now_unix_millis).await
    }
    async fn expire_ban(
        &self,
        operation: &bace_persistence::AccountBanOperation,
    ) -> Result<bace_persistence::AccountBanReceipt, Self::Error> {
        self.accounts.expire_ban(operation).await
    }
}

struct BanAccounts {
    accounts: Accounts,
    verdict: Mutex<bace_persistence::AccountBanVerdict>,
    expiry_ops: Mutex<Vec<bace_persistence::AccountBanOperation>>,
}
impl AccountRepository for BanAccounts {
    type Error = std::io::Error;
    async fn find_by_name(&self, name: &AccountName) -> Result<Option<AccountRecord>, Self::Error> {
        self.accounts.find_by_name(name).await
    }
    async fn create(&self, account: NewAccount) -> Result<CreateAccountOutcome, Self::Error> {
        self.accounts.create(account).await
    }
}
impl BanRepository for BanAccounts {
    async fn ban_verdict(
        &self,
        _: AccountId,
        _: i64,
    ) -> Result<bace_persistence::AccountBanVerdict, Self::Error> {
        Ok(self.verdict.lock().unwrap().clone())
    }
    async fn expire_ban(
        &self,
        op: &bace_persistence::AccountBanOperation,
    ) -> Result<bace_persistence::AccountBanReceipt, Self::Error> {
        self.expiry_ops.lock().unwrap().push(op.clone());
        *self.verdict.lock().unwrap() = bace_persistence::AccountBanVerdict::Allowed;
        Ok(bace_persistence::AccountBanReceipt {
            operation_id: op.operation_id,
            account_id: op.account_id,
            account_revision: op.expected_revision + 1,
            ban: None,
        })
    }
}

#[tokio::test]
async fn authenticated_ban_rejects_before_world_admission_and_expired_ban_clears() {
    use bace_persistence::{AccountBanRecord, AccountBanVerdict};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let record = AccountBanRecord {
        account_id: 1,
        account_revision: 7,
        started_unix_millis: now - 1_000,
        expires_unix_millis: now + 60_000,
        issuer_account_id: Some(2),
        reason: Some("source reason".into()),
    };
    let accounts = Arc::new(BanAccounts {
        accounts: Accounts(Mutex::new(None)),
        verdict: Mutex::new(AccountBanVerdict::Banned(record.clone())),
        expiry_ops: Mutex::new(Vec::new()),
    });
    let mut pool = AuthenticationPool::spawn(
        accounts.clone(),
        &NetworkConfig::default(),
        AccountConfig::default(),
    )
    .unwrap();
    pool.try_submit(job(1, "synthetic-password")).unwrap();
    assert!(matches!(
        pool.next().await.unwrap().result,
        Err(AuthenticationFailure::Banned { seconds_remaining: 1..=60, reason }) if reason == "source reason"
    ));
    assert!(accounts.expiry_ops.lock().unwrap().is_empty());
    pool.try_submit(job(3, "wrong-password")).unwrap();
    assert!(matches!(
        pool.next().await.unwrap().result,
        Err(AuthenticationFailure::Rejected)
    ));

    *accounts.verdict.lock().unwrap() = AccountBanVerdict::Expired(AccountBanRecord {
        expires_unix_millis: now - 1,
        ..record
    });
    pool.try_submit(job(2, "synthetic-password")).unwrap();
    assert!(pool.next().await.unwrap().result.is_ok());
    let operations = accounts.expiry_ops.lock().unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].expected_revision, 7);
    assert!(matches!(
        operations[0].change,
        bace_persistence::AccountBanChange::Expire { .. }
    ));
}
#[tokio::test]
async fn panic_drain_preserves_other_results_and_identifies_unresolved_session() {
    let config = NetworkConfig {
        authentication_workers: 1,
        ..NetworkConfig::default()
    };
    let pool = AuthenticationPool::spawn(
        Arc::new(PanickingAccounts {
            accounts: Accounts(Mutex::new(None)),
            finds: std::sync::atomic::AtomicUsize::new(0),
        }),
        &config,
        AccountConfig::default(),
    )
    .unwrap();
    pool.try_submit(job(1, "synthetic-password")).unwrap();
    pool.try_submit(job(2, "synthetic-password")).unwrap();
    let drain = pool.shutdown().await;
    assert_eq!(drain.panicked_workers, 1);
    assert_eq!(drain.completions.len(), 1);
    assert!(drain.completions[0].result.is_ok());
    assert_eq!(
        drain.unrecovered_sessions,
        [SessionKey {
            id: 0,
            generation: 2
        }]
    );
    assert!(drain.passwords_drained);
}

#[test]
fn cancelled_password_wait_still_reserves_budget_until_blocking_work_finishes() {
    use bace_runtime::authentication::PasswordExecutor;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        let executor = PasswordExecutor::new(1).unwrap();
        let worker = executor.clone();
        let task = tokio::spawn(async move { worker.hash(b"synthetic-password".to_vec()).await });
        tokio::task::yield_now().await;
        task.abort();
        let _ = task.await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), executor.wait_idle())
                .await
                .is_err()
        );
        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), executor.wait_idle())
            .await
            .unwrap()
            .unwrap();
    });
}
