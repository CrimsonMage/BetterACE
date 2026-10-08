//! CAS-fenced account bans, exact replay and bounded authentication reads.
use crate::{PgStore, StoreError};
use bace_persistence::{
    AccountBanChange, AccountBanListEntry, AccountBanOperation, AccountBanReceipt,
    AccountBanRecord, AccountBanVerdict,
};
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};

impl PgStore {
    /// Resolve an authenticated account ID with an explicit caller clock. An
    /// expired ban must be cleared through `apply_account_ban` before login.
    pub async fn account_ban_verdict(
        &self,
        account_id: u64,
        now_unix_millis: i64,
    ) -> Result<AccountBanVerdict, StoreError> {
        let id = account_id_sql(account_id)?;
        if now_unix_millis < 0 {
            return Err(StoreError::Invalid("account ban clock"));
        }
        let row = sqlx::query("SELECT id AS account_id,revision AS account_revision,disabled,ban_started_unix_millis AS started_unix_millis,ban_expires_unix_millis AS expires_unix_millis,ban_issuer_account_id AS issuer_account_id,ban_reason AS reason FROM accounts WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(AccountBanVerdict::Missing);
        };
        if row.try_get::<bool, _>("disabled")? {
            return Ok(AccountBanVerdict::Disabled);
        }
        let Some(record) = ban_record(&row)? else {
            return Ok(AccountBanVerdict::Allowed);
        };
        if now_unix_millis >= record.expires_unix_millis {
            Ok(AccountBanVerdict::Expired(record))
        } else {
            Ok(AccountBanVerdict::Banned(record))
        }
    }

    /// Exact command identity is journaled with the account revision change.
    /// A same-ID replay returns its original receipt even after later changes.
    pub async fn apply_account_ban(
        &self,
        op: &AccountBanOperation,
    ) -> Result<AccountBanReceipt, StoreError> {
        let digest = fingerprint(op)?;
        let account_id = account_id_sql(op.account_id)?;
        let expected_revision = account_id_sql(op.expected_revision)?;
        let mut tx = self.pool.begin().await?;
        let lock_key = i32::from_le_bytes(op.operation_id[..4].try_into().expect("four bytes"));
        sqlx::query("SELECT pg_advisory_xact_lock(42812023,$1)")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query("SELECT fingerprint,account_id, result_revision AS account_revision,result_started_unix_millis AS started_unix_millis,result_expires_unix_millis AS expires_unix_millis,result_issuer_account_id AS issuer_account_id,result_reason AS reason FROM staff_account_ban_operations WHERE operation_id=$1")
            .bind(op.operation_id.as_slice())
            .fetch_optional(&mut *tx)
            .await?
        {
            if row.try_get::<Vec<u8>, _>("fingerprint")? != digest {
                return Err(StoreError::OperationMismatch);
            }
            let receipt = receipt(op.operation_id, &row)?;
            tx.commit().await.map_err(crate::store::commit_error)?;
            return Ok(receipt);
        }
        let row = match &op.change {
            AccountBanChange::Ban {
                started_unix_millis,
                expires_unix_millis,
                reason,
            } => sqlx::query("UPDATE accounts SET ban_started_unix_millis=$3,ban_expires_unix_millis=$4,ban_issuer_account_id=$5,ban_reason=COALESCE($6,ban_reason),revision=revision+1 WHERE id=$1 AND revision=$2 AND revision<9223372036854775807 RETURNING id AS account_id,revision AS account_revision,ban_started_unix_millis AS started_unix_millis,ban_expires_unix_millis AS expires_unix_millis,ban_issuer_account_id AS issuer_account_id,ban_reason AS reason")
                .bind(account_id)
                .bind(expected_revision)
                .bind(started_unix_millis)
                .bind(expires_unix_millis)
                .bind(op.issuer_account_id.map(account_id_sql).transpose()?)
                .bind(reason)
                .fetch_optional(&mut *tx)
                .await?,
            AccountBanChange::Unban => sqlx::query("UPDATE accounts SET ban_started_unix_millis=NULL,ban_expires_unix_millis=NULL,ban_issuer_account_id=NULL,ban_reason=NULL,revision=revision+1 WHERE id=$1 AND revision=$2 AND revision<9223372036854775807 AND ban_expires_unix_millis IS NOT NULL RETURNING id AS account_id,revision AS account_revision,ban_started_unix_millis AS started_unix_millis,ban_expires_unix_millis AS expires_unix_millis,ban_issuer_account_id AS issuer_account_id,ban_reason AS reason")
                .bind(account_id)
                .bind(expected_revision)
                .fetch_optional(&mut *tx)
                .await?,
            AccountBanChange::Expire { now_unix_millis } => sqlx::query("UPDATE accounts SET ban_started_unix_millis=NULL,ban_expires_unix_millis=NULL,ban_issuer_account_id=NULL,ban_reason=NULL,revision=revision+1 WHERE id=$1 AND revision=$2 AND revision<9223372036854775807 AND ban_expires_unix_millis<=$3 RETURNING id AS account_id,revision AS account_revision,ban_started_unix_millis AS started_unix_millis,ban_expires_unix_millis AS expires_unix_millis,ban_issuer_account_id AS issuer_account_id,ban_reason AS reason")
                .bind(account_id)
                .bind(expected_revision)
                .bind(now_unix_millis)
                .fetch_optional(&mut *tx)
                .await?,
        }
        .ok_or(StoreError::OwnershipConflict)?;
        let receipt = receipt(op.operation_id, &row)?;
        sqlx::query("INSERT INTO staff_account_ban_operations(operation_id,fingerprint,account_id,result_revision,result_started_unix_millis,result_expires_unix_millis,result_issuer_account_id,result_reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(op.operation_id.as_slice())
            .bind(digest.as_slice())
            .bind(account_id)
            .bind(receipt.account_revision as i64)
            .bind(receipt.ban.as_ref().map(|ban| ban.started_unix_millis))
            .bind(receipt.ban.as_ref().map(|ban| ban.expires_unix_millis))
            .bind(receipt.ban.as_ref().and_then(|ban| ban.issuer_account_id).map(account_id_sql).transpose()?)
            .bind(receipt.ban.as_ref().and_then(|ban| ban.reason.as_deref()))
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(receipt)
    }

    /// Active bans only, keyset-paged by canonical account name. The cursor is
    /// returned as an entry's `canonical_name`; never materialize the full list.
    pub async fn list_active_account_bans(
        &self,
        now_unix_millis: i64,
        after_name: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AccountBanListEntry>, StoreError> {
        if now_unix_millis < 0
            || !(1..=100).contains(&limit)
            || after_name.is_some_and(|name| {
                name.is_empty() || name.len() > 200 || name.as_bytes().contains(&0)
            })
        {
            return Err(StoreError::Invalid("account ban list bounds"));
        }
        let rows = sqlx::query("SELECT a.id AS account_id,a.revision AS account_revision,a.canonical_name,issuer.canonical_name AS issuer_canonical_name,a.ban_started_unix_millis AS started_unix_millis,a.ban_expires_unix_millis AS expires_unix_millis,a.ban_issuer_account_id AS issuer_account_id,a.ban_reason AS reason FROM accounts a LEFT JOIN accounts issuer ON issuer.id=a.ban_issuer_account_id WHERE a.ban_expires_unix_millis>$1 AND ($2::text IS NULL OR a.canonical_name>$2) ORDER BY a.canonical_name LIMIT $3")
            .bind(now_unix_millis)
            .bind(after_name)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let canonical_name: String = row.try_get("canonical_name")?;
                if canonical_name.is_empty() || canonical_name.len() > 200 {
                    return Err(StoreError::Invalid("account ban list name"));
                }
                Ok(AccountBanListEntry {
                    canonical_name,
                    issuer_canonical_name: row.try_get("issuer_canonical_name")?,
                    ban: ban_record(&row)?
                        .ok_or(StoreError::Invalid("active account ban missing"))?,
                })
            })
            .collect()
    }
}

fn receipt(operation_id: [u8; 16], row: &PgRow) -> Result<AccountBanReceipt, StoreError> {
    Ok(AccountBanReceipt {
        operation_id,
        account_id: positive(row.try_get("account_id")?)?,
        account_revision: positive(row.try_get("account_revision")?)?,
        ban: ban_record(row)?,
    })
}

fn ban_record(row: &PgRow) -> Result<Option<AccountBanRecord>, StoreError> {
    let start: Option<i64> = row.try_get("started_unix_millis")?;
    let expiry: Option<i64> = row.try_get("expires_unix_millis")?;
    let issuer: Option<i64> = row.try_get("issuer_account_id")?;
    let reason: Option<String> = row.try_get("reason")?;
    match (start, expiry) {
        (None, None) if issuer.is_none() && reason.is_none() => Ok(None),
        (Some(started_unix_millis), Some(expires_unix_millis))
            if started_unix_millis >= 0
                && expires_unix_millis >= started_unix_millis
                && reason
                    .as_ref()
                    .is_none_or(|r| !r.is_empty() && r.len() <= 2048) =>
        {
            Ok(Some(AccountBanRecord {
                account_id: positive(row.try_get("account_id")?)?,
                account_revision: positive(row.try_get("account_revision")?)?,
                started_unix_millis,
                expires_unix_millis,
                issuer_account_id: issuer.map(positive).transpose()?,
                reason,
            }))
        }
        _ => Err(StoreError::Invalid("account ban record")),
    }
}

fn positive(n: i64) -> Result<u64, StoreError> {
    u64::try_from(n)
        .ok()
        .filter(|n| *n > 0)
        .ok_or(StoreError::Invalid("account ban identity/revision"))
}

fn account_id_sql(n: u64) -> Result<i64, StoreError> {
    i64::try_from(n)
        .ok()
        .filter(|n| *n > 0)
        .ok_or(StoreError::Invalid("account ban identity/revision"))
}

fn fingerprint(op: &AccountBanOperation) -> Result<[u8; 32], StoreError> {
    if op.operation_id == [0; 16]
        || op.account_id == 0
        || op.expected_revision == 0
        || op.expected_revision >= i64::MAX as u64
    {
        return Err(StoreError::Invalid("account ban operation identity"));
    }
    account_id_sql(op.account_id)?;
    op.issuer_account_id.map(account_id_sql).transpose()?;
    let mut hash = Sha256::new();
    hash.update(b"BetterACE-account-ban-v1");
    hash.update(op.account_id.to_le_bytes());
    hash.update(op.expected_revision.to_le_bytes());
    hash.update(op.issuer_account_id.unwrap_or(0).to_le_bytes());
    match &op.change {
        AccountBanChange::Ban {
            started_unix_millis,
            expires_unix_millis,
            reason,
        } => {
            if *started_unix_millis < 0 || *expires_unix_millis < *started_unix_millis {
                return Err(StoreError::Invalid("account ban time"));
            }
            if reason.as_ref().is_some_and(|r| {
                r.is_empty() || r.trim().is_empty() || r.len() > 2048 || r.as_bytes().contains(&0)
            }) {
                return Err(StoreError::Invalid("account ban reason"));
            }
            hash.update([0]);
            hash.update(started_unix_millis.to_le_bytes());
            hash.update(expires_unix_millis.to_le_bytes());
            match reason {
                Some(reason) => {
                    hash.update([1]);
                    hash.update((reason.len() as u32).to_le_bytes());
                    hash.update(reason.as_bytes());
                }
                None => hash.update([0]),
            }
        }
        AccountBanChange::Unban => hash.update([1]),
        AccountBanChange::Expire { now_unix_millis } => {
            if op.issuer_account_id.is_some() || *now_unix_millis < 0 {
                return Err(StoreError::Invalid("account ban expiry"));
            }
            hash.update([2]);
            hash.update(now_unix_millis.to_le_bytes());
        }
    }
    Ok(hash.finalize().into())
}
