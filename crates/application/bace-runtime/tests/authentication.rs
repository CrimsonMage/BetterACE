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
