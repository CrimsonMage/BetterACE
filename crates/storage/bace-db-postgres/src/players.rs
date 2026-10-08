use crate::{PgStore, StoreError};
use bace_persistence::{CharacterLease, OwnershipState, PlayerSummary, SaveSnapshot};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};
use sha2::{Digest, Sha256};
use sqlx::Row;

impl PgStore {
    /// Bounded durable reservation for starting gear, spawns, corpses and loot.
    /// Sequence values are never rolled back or recycled after cancellation.
    pub async fn allocate_dynamic_ids(&self, count: u16) -> Result<Vec<u32>, StoreError> {
        if count == 0 || count > 1024 {
            return Err(StoreError::Invalid("dynamic ID allocation count"));
        }
        let values: Vec<i64> = sqlx::query_scalar("WITH allocated AS MATERIALIZED (SELECT nextval('dynamic_object_ids') AS id FROM generate_series(1,$1)) SELECT id FROM allocated WHERE NOT EXISTS(SELECT 1 FROM entity_snapshots WHERE object_id=allocated.id) ORDER BY id")
            .bind(i32::from(count)).fetch_all(&self.pool).await?;
        if values.len() != usize::from(count) {
            return Err(StoreError::Invalid(
                "reserved dynamic ID conflicts with previously imported state; retry allocation",
            ));
        }
        values
            .into_iter()
            .map(|id| u32::try_from(id).map_err(|_| StoreError::Invalid("dynamic ID exhausted")))
            .collect()
    }
    /// Server-only identity allocation. A failed creation may leave a harmless gap.
    pub async fn allocate_player_id(&self) -> Result<u32, StoreError> {
        let id: i64 = sqlx::query_scalar("SELECT nextval('player_object_ids')")
            .fetch_one(&self.pool)
            .await?;
        u32::try_from(id).map_err(|_| StoreError::Invalid("player ID exhausted"))
    }

    /// Atomic account/name/slot reservation, frozen character and starting items.
    /// Creation rules, appearance and starting-gear eligibility are checked by gameplay.
    /// The server-allocated ID also names the durable creation receipt. An exact
    /// retry confirms creation without another write; the returned initial lease
    /// is historical, and login must acquire the current stored ownership epoch.
    pub async fn create_player(
        &self,
        player: &PlayerSaveV1,
        slot: u16,
        maximum_slots: u16,
        items: &[(EntitySaveV1, u32)],
    ) -> Result<CharacterLease, StoreError> {
        if maximum_slots == 0 || maximum_slots > 100 || slot >= maximum_slots || items.len() > 1023
        {
            return Err(StoreError::Invalid(
                "player slot or starting inventory limit",
            ));
        }
        let bytes = player
            .encode()
            .map_err(|_| StoreError::Invalid("frozen player save"))?;
        let mut snapshots = Vec::with_capacity(items.len() + 1);
        snapshots.push(SaveSnapshot {
            object_id: player.entity.object_id,
            mutation_revision: player.entity.mutation_revision,
            expected_version: 0,
            bytes,
        });
        let mut slots = std::collections::BTreeSet::new();
        for (item, item_slot) in items {
            if !slots.insert(*item_slot) || !(0x8000_0000..=0xffff_fffe).contains(&item.object_id) {
                return Err(StoreError::Invalid("starting item ID or duplicate slot"));
            }
            snapshots.push(SaveSnapshot {
                object_id: item.object_id,
                mutation_revision: item.mutation_revision,
                expected_version: 0,
                bytes: item
                    .encode_item()
                    .map_err(|_| StoreError::Invalid("frozen item save"))?,
            });
        }
        crate::writes::validate(&snapshots)?;
        let mut hash = Sha256::new();
        hash.update(b"betterace-character-create-v1");
        hash.update(slot.to_le_bytes());
        let mut ordered: Vec<_> = snapshots.iter().collect();
        ordered.sort_by_key(|snapshot| snapshot.object_id);
        for snapshot in ordered {
            hash.update(snapshot.object_id.to_le_bytes());
            hash.update((snapshot.bytes.len() as u64).to_le_bytes());
            hash.update(&snapshot.bytes);
            if let Some((_, item_slot)) = items
                .iter()
                .find(|(item, _)| item.object_id == snapshot.object_id)
            {
                hash.update(item_slot.to_le_bytes());
            }
        }
        let fingerprint: [u8; 32] = hash.finalize().into();
        let operation_id = format!("character-create:{}", player.entity.object_id);
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(&operation_id).bind(fingerprint.as_slice()).execute(&mut *tx).await?.rows_affected();
        if inserted == 0 {
            let old: Vec<u8> = sqlx::query_scalar(
                "SELECT request_fingerprint FROM durable_operations WHERE operation_id=$1",
            )
            .bind(&operation_id)
            .fetch_one(&mut *tx)
            .await?;
            if old != fingerprint {
                return Err(StoreError::OperationMismatch);
            }
            tx.commit().await.map_err(crate::store::commit_error)?;
            return Ok(CharacterLease {
                character_id: player.entity.object_id,
                epoch: 0,
                state: OwnershipState::Offline,
            });
        }
        // Serialize slot/name lifecycle for this account without blocking other accounts.
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT NOT disabled FROM accounts WHERE id=$1 FOR UPDATE")
                .bind(player.account_id as i64)
                .fetch_optional(&mut *tx)
                .await?;
        if enabled != Some(true) {
            return Err(StoreError::Invalid("account missing or disabled"));
        }
        crate::writes::write_all(&mut tx, &snapshots).await?;
        sqlx::query("INSERT INTO character_ownership(character_id) VALUES($1)")
            .bind(i64::from(player.entity.object_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO players(object_id,account_id,name,canonical_name,slot) VALUES($1,$2,$3,$4,$5)")
            .bind(i64::from(player.entity.object_id)).bind(player.account_id as i64)
            .bind(&player.name).bind(player.name.to_lowercase()).bind(slot as i16).execute(&mut *tx).await?;
        for (item, item_slot) in items {
            sqlx::query("INSERT INTO item_ownership(item_id,container_id,slot) VALUES($1,$2,$3)")
                .bind(i64::from(item.object_id))
                .bind(i64::from(player.entity.object_id))
                .bind(i64::from(*item_slot))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(CharacterLease {
            character_id: player.entity.object_id,
            epoch: 0,
            state: OwnershipState::Offline,
        })
    }

    pub async fn players_for_account(
        &self,
        account_id: u64,
    ) -> Result<Vec<PlayerSummary>, StoreError> {
        let account = i64::try_from(account_id).map_err(|_| StoreError::Invalid("account ID"))?;
        let rows = sqlx::query("SELECT object_id,account_id,name,slot,is_plussed FROM players WHERE account_id=$1 ORDER BY slot LIMIT 100")
            .bind(account).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(PlayerSummary {
                    is_plussed: r.get("is_plussed"),
                    object_id: u32::try_from(r.get::<i64, _>("object_id"))
                        .map_err(|_| StoreError::Invalid("player ID"))?,
                    account_id,
                    name: r.get("name"),
                    slot: r.get::<i16, _>("slot") as u16,
                })
            })
            .collect()
    }

    /// Identity check for authenticated character selection, before acquiring a lease.
    pub async fn account_owns_player(
        &self,
        account_id: u64,
        character_id: u32,
    ) -> Result<bool, StoreError> {
        let account = i64::try_from(account_id).map_err(|_| StoreError::Invalid("account ID"))?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM players WHERE account_id=$1 AND object_id=$2)",
        )
        .bind(account)
        .bind(i64::from(character_id))
        .fetch_one(&self.pool)
        .await?)
    }
}

impl PgStore {
    /// Read the display flag only under the exact loading/online ownership fence.
    /// This flag does not authorize any staff command or grant an access level.
    pub async fn player_is_plussed(
        &self,
        lease: CharacterLease,
        account_id: u64,
    ) -> Result<bool, StoreError> {
        if !matches!(
            lease.state,
            OwnershipState::Loading | OwnershipState::Online
        ) {
            return Err(StoreError::OwnershipConflict);
        }
        let account = i64::try_from(account_id).map_err(|_| StoreError::Invalid("account ID"))?;
        let mut tx = self.pool.begin().await?;
        crate::ownership::check_lease(&mut tx, lease).await?;
        let value: bool = sqlx::query_scalar(
            "SELECT is_plussed FROM players WHERE object_id=$1 AND account_id=$2",
        )
        .bind(i64::from(lease.character_id))
        .bind(account)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::OwnershipConflict)?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(value)
    }
}
