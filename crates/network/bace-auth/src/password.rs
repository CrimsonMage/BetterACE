use crate::AuthError;
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rand_core::{OsRng, RngCore};
use std::sync::atomic::{AtomicUsize, Ordering};

const FRESH_MEMORY_KIB: u32 = 19 * 1024;
const MAX_MEMORY_KIB: u32 = 64 * 1024;
const MAX_PASSWORD_BYTES: usize = 1024;
const MAX_PHC_BYTES: usize = 512;

/// Validated Argon2id PHC record, with salt and costs embedded. Debug output
/// deliberately omits the hash; callers explicitly obtain storage bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct PasswordHashRecord(String);
impl std::fmt::Debug for PasswordHashRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PasswordHashRecord([redacted])")
    }
}
impl PasswordHashRecord {
    pub fn parse(value: &str) -> Result<Self, AuthError> {
        validate_phc(value)?;
        Ok(Self(value.to_owned()))
    }
    pub fn as_phc(&self) -> &str {
        &self.0
    }
}
fn validate_phc(value: &str) -> Result<PasswordHash<'_>, AuthError> {
    if value.len() > MAX_PHC_BYTES {
        return Err(AuthError::InvalidPasswordHash);
    }
    let parsed = PasswordHash::new(value).map_err(|_| AuthError::InvalidPasswordHash)?;
    if parsed.algorithm.as_str() != "argon2id"
        || parsed.version != Some(19)
        || parsed.params.iter().count() != 3
        || parsed.params.get_decimal("m").is_none()
        || parsed.params.get_decimal("t").is_none()
        || parsed.params.get_decimal("p").is_none()
    {
        return Err(AuthError::InvalidPasswordHash);
    }
    let params = Params::try_from(&parsed).map_err(|_| AuthError::InvalidPasswordHash)?;
    if !(FRESH_MEMORY_KIB..=MAX_MEMORY_KIB).contains(&params.m_cost())
        || !(2..=4).contains(&params.t_cost())
        || !(1..=4).contains(&params.p_cost())
    {
        return Err(AuthError::HashCostLimit);
    }
    if parsed.hash.as_ref().map(|hash| hash.len()) != Some(32) {
        return Err(AuthError::InvalidPasswordHash);
    }
    let mut salt = [0u8; 64];
    let salt_len = parsed
        .salt
        .ok_or(AuthError::InvalidPasswordHash)?
        .decode_b64(&mut salt)
        .map_err(|_| AuthError::InvalidPasswordHash)?
        .len();
    if !(16..=32).contains(&salt_len) {
        return Err(AuthError::InvalidPasswordHash);
    }
    Ok(parsed)
}
fn check_password(password: &[u8]) -> Result<(), AuthError> {
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        Err(AuthError::InvalidPasswordLength)
    } else {
        Ok(())
    }
}
/// Synchronous memory-hard work. Share ONE instance across authentication
/// workers; clones/independent instances would have independent budgets.
/// The runtime MUST execute this outside async reactor and simulation threads.
pub struct PasswordService {
    active: AtomicUsize,
    maximum: usize,
}
impl PasswordService {
    pub fn new(maximum_concurrent: usize) -> Result<Self, AuthError> {
        if !(1..=16).contains(&maximum_concurrent) {
            return Err(AuthError::InvalidConcurrencyLimit);
        }
        Ok(Self {
            active: AtomicUsize::new(0),
            maximum: maximum_concurrent,
        })
    }
    pub fn hash(&self, password: &[u8]) -> Result<PasswordHashRecord, AuthError> {
        check_password(password)?;
        let _permit = self.acquire()?;
        let mut raw_salt = [0u8; 16];
        OsRng
            .try_fill_bytes(&mut raw_salt)
            .map_err(|_| AuthError::RandomUnavailable)?;
        let salt = SaltString::encode_b64(&raw_salt).map_err(|_| AuthError::HashFailure)?;
        let params =
            Params::new(FRESH_MEMORY_KIB, 2, 1, Some(32)).map_err(|_| AuthError::HashFailure)?;
        let hasher = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let hash = hasher
            .hash_password(password, &salt)
            .map_err(|_| AuthError::HashFailure)?
            .to_string();
        PasswordHashRecord::parse(&hash)
    }
    pub fn verify(
        &self,
        password: &[u8],
        expected: &PasswordHashRecord,
    ) -> Result<bool, AuthError> {
        check_password(password)?;
        let parsed = validate_phc(expected.as_phc())?;
        let _permit = self.acquire()?;
        match Argon2::default().verify_password(password, &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(_) => Err(AuthError::HashFailure),
        }
    }
    /// Account access gate for login. Raw verify is also exposed for password
    /// maintenance; adapters authenticating users MUST use this disabled check.
    pub fn verify_account(
        &self,
        password: &[u8],
        account: &crate::AccountRecord,
    ) -> Result<bool, AuthError> {
        if account.disabled {
            return Ok(false);
        }
        self.verify(password, &account.password_hash)
    }
    fn acquire(&self) -> Result<Permit<'_>, AuthError> {
        self.active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < self.maximum).then_some(active + 1)
            })
            .map_err(|_| AuthError::Busy)?;
        Ok(Permit(&self.active))
    }
}
struct Permit<'a>(&'a AtomicUsize);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}
#[cfg(test)]
#[path = "password_tests.rs"]
mod tests;
