use crate::{PgStore, StoreError};
use bace_persistence::{CharacterLease, OfflineXpEvent, OwnershipState, XpReceipt};
use sqlx::Row;
impl PgStore {
    /// Durable admission independent of character ownership; exact semantic retries are safe.
    pub async fn enqueue_xp_event(&self, event: &OfflineXpEvent) -> Result<(), StoreError> {
        if event.event_id.is_empty()
            || event.event_id.len() > 128
            || event.amount == 0
            || event.source_character == 0
            || event.target_character == 0
        {
            return Err(StoreError::Invalid("XP event"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO offline_xp_events(event_id,source_character,target_character,amount) VALUES($1,$2,$3,$4::text::numeric) ON CONFLICT DO NOTHING")
            .bind(&event.event_id).bind(i64::from(event.source_character)).bind(i64::from(event.target_character)).bind(event.amount.to_string()).execute(&mut *tx).await?;
        let row=sqlx::query("SELECT source_character,target_character,amount::text AS amount FROM offline_xp_events WHERE event_id=$1").bind(&event.event_id).fetch_one(&mut *tx).await?;
        if row.get::<i64, _>("source_character") != i64::from(event.source_character)
            || row.get::<i64, _>("target_character") != i64::from(event.target_character)
            || row.get::<String, _>("amount") != event.amount.to_string()
        {
            return Err(StoreError::OperationMismatch);
        }
        tx.commit().await.map_err(crate::store::commit_error)
    }
    /// Credits the offline cache, NOT total/unassigned XP or full allegiance propagation.
    pub async fn apply_offline_xp(
        &self,
        event_id: &str,
        lease: CharacterLease,
    ) -> Result<XpReceipt, StoreError> {
        if lease.state != OwnershipState::Offline {
            return Err(StoreError::OwnershipConflict);
        }
        let mut tx = self.pool.begin().await?;
        crate::ownership::check_lease(&mut tx, lease).await?;
        let event=sqlx::query("SELECT target_character,amount::text AS amount,result_cached_xp::text AS result FROM offline_xp_events WHERE event_id=$1 FOR UPDATE").bind(event_id).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("missing XP event"))?;
        if event.get::<i64, _>("target_character") != i64::from(lease.character_id) {
            return Err(StoreError::OwnershipConflict);
        }
        let prior: Option<String> = event.get("result");
        let receipt = if let Some(prior) = prior {
            XpReceipt {
                event_id: event_id.to_owned(),
                cached_xp: prior
                    .parse()
                    .map_err(|_| StoreError::Invalid("XP receipt"))?,
                newly_applied: false,
            }
        } else {
            let cached:String=sqlx::query_scalar("UPDATE character_ownership SET cached_xp=LEAST(cached_xp+$2::text::numeric,4294967295) WHERE character_id=$1 RETURNING cached_xp::text").bind(i64::from(lease.character_id)).bind(event.get::<String,_>("amount")).fetch_one(&mut *tx).await?;
            sqlx::query(
                "UPDATE offline_xp_events SET result_cached_xp=$2::text::numeric WHERE event_id=$1",
            )
            .bind(event_id)
            .bind(&cached)
            .execute(&mut *tx)
            .await?;
            XpReceipt {
                event_id: event_id.to_owned(),
                cached_xp: cached
                    .parse()
                    .map_err(|_| StoreError::Invalid("cached XP"))?,
                newly_applied: true,
            }
        };
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(receipt)
    }
    /// Bounded recovery of admitted but unapplied events; caller routes online targets separately.
    pub async fn pending_xp_events(
        &self,
        character_id: u32,
        limit: u32,
    ) -> Result<Vec<OfflineXpEvent>, StoreError> {
        if !(1..=256).contains(&limit) {
            return Err(StoreError::Invalid("XP page limit"));
        }
        let rows=sqlx::query("SELECT event_id,source_character,amount::text AS amount FROM offline_xp_events WHERE target_character=$1 AND result_cached_xp IS NULL ORDER BY event_id LIMIT $2").bind(i64::from(character_id)).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(OfflineXpEvent {
                    event_id: r.get("event_id"),
                    source_character: r.get::<i64, _>("source_character") as u32,
                    target_character: character_id,
                    amount: r
                        .get::<String, _>("amount")
                        .parse()
                        .map_err(|_| StoreError::Invalid("XP amount"))?,
                })
            })
            .collect()
    }
}
