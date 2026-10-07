use crate::{
    AccountName, AccountRecord, AccountRepository, AuthError, CreateAccountOutcome, NewAccount,
    PasswordHashRecord,
};
use std::future::Future;

/// Runtime implementations execute memory-hard work on bounded blocking workers.
pub trait PasswordWorker: Send + Sync {
    fn hash(
        &self,
        password: Vec<u8>,
    ) -> impl Future<Output = Result<PasswordHashRecord, AuthError>> + Send;
    fn verify(
        &self,
        password: Vec<u8>,
        hash: PasswordHashRecord,
    ) -> impl Future<Output = Result<bool, AuthError>> + Send;
}

/// Credentials are never printed by derived error or request diagnostics.
pub struct PasswordCredentials {
    name: AccountName,
    password: Vec<u8>,
}

impl std::fmt::Debug for PasswordCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PasswordCredentials([redacted])")
    }
}

impl PasswordCredentials {
    pub fn new(name: &str, password: &[u8]) -> Result<Self, AuthError> {
        if password.is_empty() || password.len() > 1024 {
            return Err(AuthError::InvalidPasswordLength);
        }
        Ok(Self {
            name: AccountName::parse(name)?,
            password: password.to_vec(),
        })
    }
}

#[derive(Debug)]
pub enum LoginError<E> {
    Repository(E),
    Password(AuthError),
    Rejected,
}

impl<E: std::fmt::Display> std::fmt::Display for LoginError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Repository errors may contain connection details. Keep login-facing
        // diagnostics stable and leave adapter diagnostics at their owner.
        f.write_str(match self {
            Self::Repository(_) => "account repository unavailable",
            Self::Password(_) => "password worker unavailable",
            Self::Rejected => "login rejected",
        })
    }
}
impl<E: std::error::Error + 'static> std::error::Error for LoginError<E> {}

/// Authenticate native accounts; automatic creation always uses NewAccount's
/// Player/enabled policy. A concurrent creator never gets its hash overwritten.
/// The caller must fence completion with its session generation before login.
pub async fn authenticate<R: AccountRepository, W: PasswordWorker>(
    repository: &R,
    passwords: &W,
    credentials: PasswordCredentials,
    allow_auto_creation: bool,
) -> Result<AccountRecord, LoginError<R::Error>> {
    let mut account = repository
        .find_by_name(&credentials.name)
        .await
        .map_err(LoginError::Repository)?;
    if account.is_none() && allow_auto_creation {
        let hash = passwords
            .hash(credentials.password.clone())
            .await
            .map_err(LoginError::Password)?;
        account = match repository
            .create(NewAccount {
                name: credentials.name.clone(),
                password_hash: hash,
            })
            .await
            .map_err(LoginError::Repository)?
        {
            CreateAccountOutcome::Created(record) => Some(record),
            CreateAccountOutcome::AlreadyExists => repository
                .find_by_name(&credentials.name)
                .await
                .map_err(LoginError::Repository)?,
        };
    }
    let Some(account) = account else {
        // Pay bounded password work even for unknown accounts to avoid a cheap
        // unknown-name path. This is not a constant-time authentication claim.
        passwords
            .hash(credentials.password)
            .await
            .map_err(LoginError::Password)?;
        return Err(LoginError::Rejected);
    };
    let matches = passwords
        .verify(credentials.password, account.password_hash.clone())
        .await
        .map_err(LoginError::Password)?;
    if !matches || account.disabled || account.name != credentials.name {
        return Err(LoginError::Rejected);
    }
    Ok(account)
}
