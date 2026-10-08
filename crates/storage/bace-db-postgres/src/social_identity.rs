//! Indexed social identity lookups. No world decode or account credential projection.
use crate::{PgStore, StoreError};
use sqlx::Row;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerIdentity {
    pub object_id: u32,
    pub account_id: u64,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerIdentityQuery {
    Name(String),
    Character(u32),
}
impl PgStore {
    pub async fn lookup_player_identity(
        &self,
        query: &PlayerIdentityQuery,
    ) -> Result<Option<PlayerIdentity>, StoreError> {
        let row = match query {
            PlayerIdentityQuery::Name(name) => {
                if name.is_empty() || name.len() > 100 || name.contains('\0') {
                    return Err(StoreError::Invalid("invalid social name"));
                }
                sqlx::query("SELECT object_id,account_id,name FROM players WHERE canonical_name=$1")
                    .bind(name.trim_start_matches('+').to_lowercase())
                    .fetch_optional(&self.pool)
                    .await?
            }
            PlayerIdentityQuery::Character(id) => {
                if !(0x50000001..=0x5fffffff).contains(id) {
                    return Err(StoreError::Invalid("invalid social character identity"));
                }
                sqlx::query("SELECT object_id,account_id,name FROM players WHERE object_id=$1")
                    .bind(i64::from(*id))
                    .fetch_optional(&self.pool)
                    .await?
            }
        };
        row.map(decode_identity).transpose()
    }
}

impl PgStore {
    /// Missing rows are omitted explicitly; callers must verify every required friend.
    pub async fn lookup_player_identities(
        &self,
        ids: &[u32],
    ) -> Result<Vec<PlayerIdentity>, StoreError> {
        if ids.len() > 1024 {
            return Err(StoreError::Invalid("social identity batch capacity"));
        }
        let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() || ids.iter().any(|id| !(0x50000001..=0x5fffffff).contains(id))
        {
            return Err(StoreError::Invalid("invalid social identity batch"));
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = ids.iter().map(|id| i64::from(*id)).collect();
        let rows=sqlx::query("SELECT object_id,account_id,name FROM players WHERE object_id=ANY($1) ORDER BY object_id").bind(ids).fetch_all(&self.pool).await?;
        rows.into_iter().map(decode_identity).collect()
    }
}

fn decode_identity(row: sqlx::postgres::PgRow) -> Result<PlayerIdentity, StoreError> {
    let object_id = u32::try_from(row.try_get::<i64, _>("object_id")?)
        .map_err(|_| StoreError::Invalid("invalid stored social identity"))?;
    let account_id = u64::try_from(row.try_get::<i64, _>("account_id")?)
        .ok()
        .filter(|v| *v != 0)
        .ok_or(StoreError::Invalid("invalid stored social account"))?;
    let name: String = row.try_get("name")?;
    if !(0x50000001..=0x5fffffff).contains(&object_id)
        || name.is_empty()
        || name.len() > 100
        || name.contains('\0')
    {
        return Err(StoreError::Invalid("invalid stored social identity"));
    }
    Ok(PlayerIdentity {
        object_id,
        account_id,
        name,
    })
}
