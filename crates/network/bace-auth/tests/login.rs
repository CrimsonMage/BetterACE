use bace_auth::*;
use bace_types::AccountId;
use std::{
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

fn hash_record() -> PasswordHashRecord {
    PasswordHashRecord::parse("$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap()
}
fn record() -> AccountRecord {
    AccountRecord {
        id: AccountId(1),
        name: AccountName::parse("player").unwrap(),
        password_hash: hash_record(),
        access_level: AccessLevel::Player,
        disabled: false,
    }
}
struct Repo {
    account: Mutex<Option<AccountRecord>>,
    race: bool,
}
impl AccountRepository for Repo {
    type Error = std::io::Error;
    async fn find_by_name(&self, _: &AccountName) -> Result<Option<AccountRecord>, Self::Error> {
        Ok(self.account.lock().unwrap().clone())
    }
    async fn create(&self, _: NewAccount) -> Result<CreateAccountOutcome, Self::Error> {
        let mut account = self.account.lock().unwrap();
        if account.is_some() {
            return Ok(CreateAccountOutcome::AlreadyExists);
        }
        *account = Some(record());
        Ok(if self.race {
            CreateAccountOutcome::AlreadyExists
        } else {
            CreateAccountOutcome::Created(record())
        })
    }
}
struct Worker {
    valid: bool,
}
impl PasswordWorker for Worker {
    async fn hash(&self, _: Vec<u8>) -> Result<PasswordHashRecord, AuthError> {
        Ok(hash_record())
    }
    async fn verify(&self, _: Vec<u8>, _: PasswordHashRecord) -> Result<bool, AuthError> {
        Ok(self.valid)
    }
}
fn ready<T>(f: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    match pin!(f).poll(&mut Context::from_waker(waker)) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("test repository must complete immediately"),
    }
}
fn credentials() -> PasswordCredentials {
    PasswordCredentials::new("Player", b"synthetic").unwrap()
}
#[test]
fn auto_creation_respects_switch_and_requires_winning_password() {
    let repo = Repo {
        account: Mutex::new(None),
        race: false,
    };
    assert!(matches!(
        ready(authenticate(
            &repo,
            &Worker { valid: true },
            credentials(),
            false
        )),
        Err(LoginError::Rejected)
    ));
    assert!(repo.account.lock().unwrap().is_none());
    let account = ready(authenticate(
        &repo,
        &Worker { valid: true },
        credentials(),
        true,
    ))
    .unwrap();
    assert_eq!(account.access_level, AccessLevel::Player);
    assert!(!account.disabled);
    let race = Repo {
        account: Mutex::new(None),
        race: true,
    };
    assert!(matches!(
        ready(authenticate(
            &race,
            &Worker { valid: false },
            credentials(),
            true
        )),
        Err(LoginError::Rejected)
    ));
    assert_eq!(
        race.account.lock().unwrap().as_ref().unwrap().password_hash,
        hash_record()
    );
}
#[test]
fn disabled_and_wrong_password_never_authenticate() {
    let mut account = record();
    account.disabled = true;
    let repo = Repo {
        account: Mutex::new(Some(account)),
        race: false,
    };
    assert!(matches!(
        ready(authenticate(
            &repo,
            &Worker { valid: true },
            credentials(),
            true
        )),
        Err(LoginError::Rejected)
    ));
    assert!(!format!("{:?}", credentials()).contains("synthetic"));
}
#[test]
fn admission_is_bounded_without_extending_rejection_windows() {
    let mut attempts = LoginAttempts::new(1, 2, 60_000).unwrap();
    let a = "127.0.0.1".parse().unwrap();
    let b = "127.0.0.2".parse().unwrap();
    attempts.admit(a, 0).unwrap();
    attempts.admit(a, 1).unwrap();
    assert_eq!(attempts.admit(a, 59_999), Err(AttemptError::RateLimited));
    assert_eq!(attempts.admit(b, 59_999), Err(AttemptError::Capacity));
    attempts.admit(b, 60_000).unwrap();
    assert_eq!(attempts.tracked_addresses(), 1);
    assert_eq!(attempts.admit(b, 59_999), Err(AttemptError::InvalidClock));
}
