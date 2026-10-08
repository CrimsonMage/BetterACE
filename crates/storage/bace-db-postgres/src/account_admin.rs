//! CAS account administration with a durable, exact-request operation journal.
use crate::{PgStore, StoreError};
use bace_auth::{
    AccessLevel, AccountAdminChange, AccountAdminOperation, AccountAdminReceipt,
    AccountAdminRepository, AccountName, AccountRecord, PasswordHashRecord, VersionedAccount,
};
use bace_types::AccountId;
use sha2::{Digest, Sha256};
use sqlx::Row;
impl AccountAdminRepository for PgStore {
    type Error = StoreError;
    async fn account_for_admin(
        &self,
        name: &AccountName,
    ) -> Result<Option<VersionedAccount>, StoreError> {
        let row=sqlx::query("SELECT id,canonical_name,password_phc,access_level,disabled,revision FROM accounts WHERE canonical_name=$1").bind(name.as_str()).fetch_optional(&self.pool).await?;
        row.map(|r| {
            let id: i64 = r.try_get("id")?;
            let name: String = r.try_get("canonical_name")?;
            let hash: String = r.try_get("password_phc")?;
            let revision: i64 = r.try_get("revision")?;
            Ok(VersionedAccount {
                account: AccountRecord {
                    id: AccountId(positive(id)?),
                    name: AccountName::parse(&name)
                        .map_err(|_| StoreError::Invalid("account name"))?,
                    password_hash: PasswordHashRecord::parse(&hash)
                        .map_err(|_| StoreError::Invalid("account password record"))?,
                    access_level: access(r.try_get("access_level")?)?,
                    disabled: r.try_get("disabled")?,
                },
                revision: positive(revision)?,
            })
        })
        .transpose()
    }
    async fn apply_account_admin(
        &self,
        op: &AccountAdminOperation,
    ) -> Result<AccountAdminReceipt, StoreError> {
        let digest = fingerprint(op)?;
        let mut tx = self.pool.begin().await?;
        // A transaction-scoped deterministic lock serializes retries before journal
        // lookup; collisions only serialize unrelated operations, never affect identity.
        let key = i32::from_le_bytes(
            op.operation_id[..4]
                .try_into()
                .map_err(|_| StoreError::Invalid("operation ID"))?,
        );
        sqlx::query("SELECT pg_advisory_xact_lock(42812015,$1)")
            .bind(key)
            .execute(&mut *tx)
            .await?;
        if let Some(row)=sqlx::query("SELECT fingerprint,account_id,canonical_name,result_revision,result_access,result_disabled FROM staff_account_operations WHERE operation_id=$1").bind(op.operation_id.as_slice()).fetch_optional(&mut *tx).await? {
   let prior:Vec<u8>=row.try_get("fingerprint")?;if prior!=digest{return Err(StoreError::OperationMismatch)}
   let receipt=receipt(op.operation_id,&row)?;tx.commit().await.map_err(crate::store::commit_error)?;return Ok(receipt)
  }
        let row=match &op.change {
   AccountAdminChange::Create{name,password_hash,access}=>sqlx::query("INSERT INTO accounts(canonical_name,password_phc,access_level,disabled,revision) VALUES($1,$2,$3,false,1) ON CONFLICT(canonical_name) DO NOTHING RETURNING id AS account_id,canonical_name,revision AS result_revision,access_level AS result_access,disabled AS result_disabled").bind(name.as_str()).bind(password_hash.as_phc()).bind(*access as i16).fetch_optional(&mut *tx).await?,
   AccountAdminChange::Update{account,expected_revision,access,password_hash}=>sqlx::query("UPDATE accounts SET access_level=COALESCE($3,access_level),password_phc=COALESCE($4,password_phc),revision=revision+1 WHERE id=$1 AND revision=$2 AND revision<9223372036854775807 RETURNING id AS account_id,canonical_name,revision AS result_revision,access_level AS result_access,disabled AS result_disabled").bind(i64::try_from(account.0).map_err(|_|StoreError::Invalid("account ID"))?).bind(i64::try_from(*expected_revision).map_err(|_|StoreError::Invalid("account revision"))?).bind(access.map(|a|a as i16)).bind(password_hash.as_ref().map(|h|h.as_phc())).fetch_optional(&mut *tx).await?,
  }.ok_or(StoreError::OwnershipConflict)?;
        let receipt = receipt(op.operation_id, &row)?;
        sqlx::query("INSERT INTO staff_account_operations(operation_id,fingerprint,issuer,account_id,canonical_name,result_revision,result_access,result_disabled) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(op.operation_id.as_slice()).bind(digest.as_slice()).bind(op.issuer.map(|i|i64::try_from(i.0)).transpose().map_err(|_|StoreError::Invalid("issuer ID"))?).bind(receipt.account.0 as i64).bind(receipt.name.as_str()).bind(receipt.revision as i64).bind(receipt.access as i16).bind(receipt.disabled).execute(&mut *tx).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(receipt)
    }
}
fn positive(n: i64) -> Result<u64, StoreError> {
    u64::try_from(n)
        .ok()
        .filter(|n| *n > 0)
        .ok_or(StoreError::Invalid("account identity/revision"))
}
fn access(n: i16) -> Result<AccessLevel, StoreError> {
    u8::try_from(n)
        .ok()
        .and_then(|n| AccessLevel::try_from(n).ok())
        .ok_or(StoreError::Invalid("account access"))
}
fn receipt(
    operation_id: [u8; 16],
    r: &sqlx::postgres::PgRow,
) -> Result<AccountAdminReceipt, StoreError> {
    let name: String = r.try_get("canonical_name")?;
    Ok(AccountAdminReceipt {
        operation_id,
        account: AccountId(positive(r.try_get("account_id")?)?),
        name: AccountName::parse(&name).map_err(|_| StoreError::Invalid("account name"))?,
        revision: positive(r.try_get("result_revision")?)?,
        access: access(r.try_get("result_access")?)?,
        disabled: r.try_get("result_disabled")?,
    })
}
fn fingerprint(op: &AccountAdminOperation) -> Result<[u8; 32], StoreError> {
    if op.operation_id == [0; 16] || op.issuer.is_some_and(|i| i.0 == 0 || i.0 > i64::MAX as u64) {
        return Err(StoreError::Invalid("account operation identity"));
    }
    let mut hash = Sha256::new();
    hash.update(b"BetterACE-account-admin-v1");
    hash.update(op.issuer.map(|i| i.0).unwrap_or(0).to_le_bytes());
    let mut string = |s: &str| {
        hash.update((s.len() as u32).to_le_bytes());
        hash.update(s.as_bytes());
    };
    match &op.change {
        AccountAdminChange::Create {
            name,
            password_hash,
            access,
        } => {
            string(name.as_str());
            string(password_hash.as_phc());
            hash.update([0, *access as u8]);
        }
        AccountAdminChange::Update {
            account,
            expected_revision,
            access,
            password_hash,
        } => {
            if account.0 == 0
                || account.0 > i64::MAX as u64
                || *expected_revision == 0
                || *expected_revision >= i64::MAX as u64
                || (access.is_none() && password_hash.is_none())
            {
                return Err(StoreError::Invalid("account update"));
            }
            if let Some(h) = password_hash {
                string(h.as_phc())
            } else {
                string("")
            }
            hash.update([1, access.map(|a| a as u8).unwrap_or(255)]);
            hash.update(account.0.to_le_bytes());
            hash.update(expected_revision.to_le_bytes());
        }
    }
    Ok(hash.finalize().into())
}
