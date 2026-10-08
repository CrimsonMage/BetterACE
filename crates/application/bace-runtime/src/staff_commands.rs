//! Durable account command lane. Admission rechecks the exact pinned catalog;
//! prepared hashes and CAS revisions stay fixed across uncertain retries.
use bace_admin::{
    AccountOperation, StaffOperation, authorize_command, parse_command, prepare_staff_operation,
};
use bace_auth::{
    AccessLevel, AccountAdminChange, AccountAdminOperation, AccountAdminReceipt,
    AccountAdminRepository, AccountName, PasswordWorker, StaffPrincipal,
};
use bace_db_postgres::{PgStore, StoreError};
use bace_types::AccountId;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
#[derive(Clone, Debug)]
pub enum StaffCommandIdentity {
    /// Supplied only after independent host-operator authentication.
    Host,
    /// Supplied only by the bound authenticated session and staff registry.
    Game {
        account: AccountId,
        name: AccountName,
        principal: StaffPrincipal,
    },
}
#[derive(Debug, thiserror::Error)]
pub enum StaffCommandError {
    #[error("staff command capacity exhausted")]
    Busy,
    #[error("staff command is invalid or not authorized")]
    Forbidden,
    #[error("staff command needs another subsystem owner")]
    WrongOwner,
    #[error("target account is missing or concurrently changed")]
    Missing,
    #[error("account credential verification failed")]
    Credentials,
    #[error("account operation failed: {0}")]
    Store(#[from] StoreError),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffAccountView {
    pub account: AccountId,
    pub name: AccountName,
    pub access: AccessLevel,
    pub disabled: bool,
    pub revision: u64,
}
pub enum PreparedStaffAccount {
    Read(StaffAccountView),
    Write(PendingStaffAccount),
}
pub struct StaffAccountService {
    store: PgStore,
    passwords: crate::authentication::PasswordExecutor,
    capacity: Arc<Semaphore>,
    default_access: AccessLevel,
}
pub struct PendingStaffAccount {
    store: PgStore,
    operation: AccountAdminOperation,
    permit: Option<OwnedSemaphorePermit>,
    committed: Option<AccountAdminReceipt>,
    uncertain: bool,
}
impl StaffAccountService {
    pub fn new(
        store: PgStore,
        passwords: crate::authentication::PasswordExecutor,
        capacity: usize,
        default_access: AccessLevel,
    ) -> Result<Self, StaffCommandError> {
        if !(1..=128).contains(&capacity) {
            return Err(StaffCommandError::Busy);
        }
        Ok(Self {
            store,
            passwords,
            capacity: Arc::new(Semaphore::new(capacity)),
            default_access,
        })
    }
    pub async fn prepare(
        &self,
        identity: &StaffCommandIdentity,
        line: &str,
        operation_id: [u8; 16],
    ) -> Result<PreparedStaffAccount, StaffCommandError> {
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| StaffCommandError::Busy)?;
        let principal = match identity {
            StaffCommandIdentity::Host => None,
            StaffCommandIdentity::Game { principal, .. } => Some(*principal),
        };
        let command = authorize_command(
            parse_command(line).map_err(|_| StaffCommandError::Forbidden)?,
            principal,
        )
        .map_err(|_| StaffCommandError::Forbidden)?;
        let Some(StaffOperation::Account(operation)) =
            prepare_staff_operation(&command).map_err(|_| StaffCommandError::Forbidden)?
        else {
            return Err(StaffCommandError::WrongOwner);
        };
        let issuer = match identity {
            StaffCommandIdentity::Host => None,
            StaffCommandIdentity::Game {
                account,
                name,
                principal,
            } => {
                let current = self
                    .store
                    .account_for_admin(name)
                    .await?
                    .ok_or(StaffCommandError::Forbidden)?;
                if current.account.id != *account
                    || current.account.disabled
                    || current.account.access_level != principal.account_access
                {
                    return Err(StaffCommandError::Forbidden);
                }
                Some(*account)
            }
        };
        let change = match operation {
            AccountOperation::Create {
                name,
                password,
                access,
            } => AccountAdminChange::Create {
                name: AccountName::parse(&name).map_err(|_| StaffCommandError::Forbidden)?,
                password_hash: self
                    .passwords
                    .hash(password.expose().as_bytes().to_vec())
                    .await
                    .map_err(|_| StaffCommandError::Credentials)?,
                access: access.unwrap_or(self.default_access),
            },
            AccountOperation::Get { name } => {
                let target = self.target(&name).await?;
                return Ok(PreparedStaffAccount::Read(StaffAccountView {
                    account: target.account.id,
                    name: target.account.name,
                    access: target.account.access_level,
                    disabled: target.account.disabled,
                    revision: target.revision,
                }));
            }
            AccountOperation::SetAccess { name, access } => {
                let target = self.target(&name).await?;
                AccountAdminChange::Update {
                    account: target.account.id,
                    expected_revision: target.revision,
                    access: Some(access),
                    password_hash: None,
                }
            }
            AccountOperation::SetPassword { name, password } => {
                let target = self.target(&name).await?;
                AccountAdminChange::Update {
                    account: target.account.id,
                    expected_revision: target.revision,
                    access: None,
                    password_hash: Some(
                        self.passwords
                            .hash(password.expose().as_bytes().to_vec())
                            .await
                            .map_err(|_| StaffCommandError::Credentials)?,
                    ),
                }
            }
            AccountOperation::ChangeOwnPassword { old, new } => {
                let StaffCommandIdentity::Game { account, name, .. } = identity else {
                    return Err(StaffCommandError::Forbidden);
                };
                let target = self
                    .store
                    .account_for_admin(name)
                    .await?
                    .ok_or(StaffCommandError::Missing)?;
                if target.account.id != *account
                    || target.account.disabled
                    || !self
                        .passwords
                        .verify(
                            old.expose().as_bytes().to_vec(),
                            target.account.password_hash,
                        )
                        .await
                        .map_err(|_| StaffCommandError::Credentials)?
                {
                    return Err(StaffCommandError::Credentials);
                }
                AccountAdminChange::Update {
                    account: *account,
                    expected_revision: target.revision,
                    access: None,
                    password_hash: Some(
                        self.passwords
                            .hash(new.expose().as_bytes().to_vec())
                            .await
                            .map_err(|_| StaffCommandError::Credentials)?,
                    ),
                }
            }
        };
        if operation_id == [0; 16] {
            return Err(StaffCommandError::Forbidden);
        }
        Ok(PreparedStaffAccount::Write(PendingStaffAccount {
            store: self.store.clone(),
            operation: AccountAdminOperation {
                operation_id,
                issuer,
                change,
            },
            permit: Some(permit),
            committed: None,
            uncertain: false,
        }))
    }
    async fn target(&self, name: &str) -> Result<bace_auth::VersionedAccount, StaffCommandError> {
        self.store
            .account_for_admin(&AccountName::parse(name).map_err(|_| StaffCommandError::Forbidden)?)
            .await?
            .ok_or(StaffCommandError::Missing)
    }
}
impl PendingStaffAccount {
    pub fn operation_id(&self) -> [u8; 16] {
        self.operation.operation_id
    }
    pub fn uncertain(&self) -> bool {
        self.uncertain
    }
    /// Cancellation leaves this caller-owned value uncertain and reserved. Retry
    /// always submits the same bytes; only an exact receipt releases capacity.
    pub async fn commit(&mut self) -> Result<AccountAdminReceipt, StaffCommandError> {
        if let Some(receipt) = &self.committed {
            return Ok(receipt.clone());
        }
        let previously_uncertain = self.uncertain;
        self.uncertain = true;
        match self.store.apply_account_admin(&self.operation).await {
            Ok(receipt) => {
                self.uncertain = false;
                self.committed = Some(receipt.clone());
                self.permit.take();
                Ok(receipt)
            }
            Err(error) => {
                self.uncertain =
                    previously_uncertain || matches!(error, StoreError::CommitUncertain(_));
                Err(StaffCommandError::Store(error))
            }
        }
    }
}
