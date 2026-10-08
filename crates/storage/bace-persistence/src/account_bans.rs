//! Exact account-ban mutations and bounded read evidence for login and staff.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBanRecord {
    pub account_id: u64,
    pub account_revision: u64,
    pub started_unix_millis: i64,
    pub expires_unix_millis: i64,
    /// None is the host console; an account ID retains historical issuer identity.
    pub issuer_account_id: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountBanVerdict {
    Missing,
    Disabled,
    Allowed,
    /// Expiry does not silently mutate storage. Login clears it with an exact
    /// Expire operation before admitting the account, as pinned ACE does.
    Expired(AccountBanRecord),
    Banned(AccountBanRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountBanChange {
    Ban {
        started_unix_millis: i64,
        expires_unix_millis: i64,
        /// None retains the prior reason on a reban, matching pinned ACE.
        reason: Option<String>,
    },
    Unban,
    Expire {
        now_unix_millis: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBanOperation {
    pub operation_id: [u8; 16],
    pub account_id: u64,
    pub expected_revision: u64,
    /// None is the host console or automatic expiry. Ban/Unban staff paths
    /// provide their authenticated issuer account when available.
    pub issuer_account_id: Option<u64>,
    pub change: AccountBanChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBanReceipt {
    pub operation_id: [u8; 16],
    pub account_id: u64,
    pub account_revision: u64,
    pub ban: Option<AccountBanRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBanListEntry {
    pub canonical_name: String,
    /// Historical issuer may no longer resolve to a current account name.
    pub issuer_canonical_name: Option<String>,
    pub ban: AccountBanRecord,
}
