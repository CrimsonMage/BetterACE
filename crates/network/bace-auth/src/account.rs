use crate::{AuthError, PasswordHashRecord};
use bace_types::AccountId;
/// Canonical fresh-account identity. Unicode lowercase is explicit and locale
/// independent; no whitespace trimming or Unicode normalization is performed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountName(String);
impl AccountName {
    pub fn parse(value: &str) -> Result<Self, AuthError> {
        if value.is_empty()
            || value.encode_utf16().count() > 50
            || value.chars().any(char::is_control)
        {
            return Err(AuthError::InvalidAccountName);
        }
        let value = value.to_lowercase();
        if value.encode_utf16().count() > 50 {
            return Err(AuthError::InvalidAccountName);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AccessLevel {
    Player = 0,
    Advocate = 1,
    Sentinel = 2,
    Envoy = 3,
    Developer = 4,
    Admin = 5,
}
impl TryFrom<u8> for AccessLevel {
    type Error = AuthError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Player),
            1 => Ok(Self::Advocate),
            2 => Ok(Self::Sentinel),
            3 => Ok(Self::Envoy),
            4 => Ok(Self::Developer),
            5 => Ok(Self::Admin),
            _ => Err(AuthError::InvalidAccessLevel),
        }
    }
}
#[derive(Clone, Debug)]
pub struct AccountRecord {
    pub id: AccountId,
    pub name: AccountName,
    pub password_hash: PasswordHashRecord,
    pub access_level: AccessLevel,
    pub disabled: bool,
}
/// Repository create MUST persist Player/disabled=false, never promote the first
/// account. Administrative provisioning is a separate explicit operation.
#[derive(Clone, Debug)]
pub struct NewAccount {
    pub name: AccountName,
    pub password_hash: PasswordHashRecord,
}
