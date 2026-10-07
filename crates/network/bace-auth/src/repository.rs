use crate::{AccountName, AccountRecord, NewAccount};
use std::{error::Error, future::Future};
#[derive(Clone, Debug)]
pub enum CreateAccountOutcome {
    Created(AccountRecord),
    AlreadyExists,
}
/// Adapter contract: name uniqueness and fresh identity allocation MUST be
/// atomic. A duplicate MUST NOT change an existing password or access level.
pub trait AccountRepository: Send + Sync {
    type Error: Error + Send + Sync + 'static;
    fn find_by_name(
        &self,
        name: &AccountName,
    ) -> impl Future<Output = Result<Option<AccountRecord>, Self::Error>> + Send;
    fn create(
        &self,
        account: NewAccount,
    ) -> impl Future<Output = Result<CreateAccountOutcome, Self::Error>> + Send;
}
