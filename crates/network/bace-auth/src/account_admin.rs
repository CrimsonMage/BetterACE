//! Explicit administrative account writes. Callers retain the prepared operation
//! across uncertain commits; password plaintext never enters this contract.
use crate::{AccessLevel, AccountName, AccountRecord, PasswordHashRecord};
use bace_types::AccountId;
use std::{error::Error, future::Future};
#[derive(Clone, Debug)]
pub struct VersionedAccount {
    pub account: AccountRecord,
    pub revision: u64,
}
#[derive(Clone, Debug)]
pub enum AccountAdminChange {
    Create {
        name: AccountName,
        password_hash: PasswordHashRecord,
        access: AccessLevel,
    },
    Update {
        account: AccountId,
        expected_revision: u64,
        access: Option<AccessLevel>,
        password_hash: Option<PasswordHashRecord>,
    },
}
#[derive(Clone, Debug)]
pub struct AccountAdminOperation {
    pub operation_id: [u8; 16],
    pub issuer: Option<AccountId>,
    pub change: AccountAdminChange,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountAdminReceipt {
    pub operation_id: [u8; 16],
    pub account: AccountId,
    pub name: AccountName,
    pub revision: u64,
    pub access: AccessLevel,
    pub disabled: bool,
}
/// Authentication and exact command authorization must precede this storage port.
/// Implementations atomically journal the exact fingerprint and resulting receipt
/// with CAS revision changes; replay returns the original receipt, never current state.
pub trait AccountAdminRepository: Send + Sync {
    type Error: Error + Send + Sync + 'static;
    fn account_for_admin(
        &self,
        name: &AccountName,
    ) -> impl Future<Output = Result<Option<VersionedAccount>, Self::Error>> + Send;
    fn apply_account_admin(
        &self,
        operation: &AccountAdminOperation,
    ) -> impl Future<Output = Result<AccountAdminReceipt, Self::Error>> + Send;
}
