use crate::{PgStore, StoreError};
use bace_auth::{
    AccessLevel, AccountName, AccountRecord, AccountRepository, CreateAccountOutcome, NewAccount,
    PasswordHashRecord,
};
use bace_types::AccountId;
use sqlx::{Row, postgres::PgRow};

impl AccountRepository for PgStore {
    type Error = StoreError;

    async fn find_by_name(&self, name: &AccountName) -> Result<Option<AccountRecord>, Self::Error> {
        let row = sqlx::query("SELECT id,canonical_name,password_phc,access_level,disabled FROM accounts WHERE canonical_name=$1")
            .bind(name.as_str()).fetch_optional(&self.pool).await?;
        row.map(decode_account).transpose()
    }

    async fn create(&self, account: NewAccount) -> Result<CreateAccountOutcome, Self::Error> {
        // Fresh accounts are always ordinary players, even when the account table is empty.
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("INSERT INTO accounts(canonical_name,password_phc,access_level,disabled) VALUES($1,$2,0,false) ON CONFLICT(canonical_name) DO NOTHING RETURNING id,canonical_name,password_phc,access_level,disabled")
            .bind(account.name.as_str()).bind(account.password_hash.as_phc()).fetch_optional(&mut *tx).await?;
        let outcome = match row {
            Some(row) => CreateAccountOutcome::Created(decode_account(row)?),
            None => CreateAccountOutcome::AlreadyExists,
        };
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(outcome)
    }
}

fn decode_account(row: PgRow) -> Result<AccountRecord, StoreError> {
    let id: i64 = row.try_get("id")?;
    let id = u64::try_from(id)
        .ok()
        .filter(|id| *id > 0)
        .ok_or(StoreError::Invalid("invalid stored account ID"))?;
    let stored_name: String = row.try_get("canonical_name")?;
    let name = AccountName::parse(&stored_name)
        .map_err(|_| StoreError::Invalid("invalid stored account name"))?;
    if name.as_str() != stored_name {
        return Err(StoreError::Invalid("stored account name is not canonical"));
    }
    let hash: String = row.try_get("password_phc")?;
    let password_hash = PasswordHashRecord::parse(&hash)
        .map_err(|_| StoreError::Invalid("invalid stored password record"))?;
    let level: i16 = row.try_get("access_level")?;
    let access_level = u8::try_from(level)
        .ok()
        .and_then(|level| AccessLevel::try_from(level).ok())
        .ok_or(StoreError::Invalid("invalid stored account access level"))?;
    Ok(AccountRecord {
        id: AccountId(id),
        name,
        password_hash,
        access_level,
        disabled: row.try_get("disabled")?,
    })
}

impl PgStore {
    /// Authenticated account eligibility evidence; NULL means historical age is unknown.
    pub async fn account_creation_time(
        &self,
        account: AccountId,
    ) -> Result<Option<i64>, StoreError> {
        let id = i64::try_from(account.0)
            .ok()
            .filter(|id| *id > 0)
            .ok_or(StoreError::Invalid("invalid account identity"))?;
        let row = sqlx::query("SELECT created_unix FROM accounts WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::Invalid("account does not exist"))?;
        row.try_get("created_unix").map_err(StoreError::from)
    }
}
