//! Bounded immutable item construction and source-origin checks for all writers.
use crate::StoreError;
use bace_persistence::SaveSnapshot;
use bace_storage_codec::ItemSaveV5;
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeMap;

pub(super) async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
    snapshots: &BTreeMap<u32, &SaveSnapshot>,
) -> Result<(), StoreError> {
    let sizes = sqlx::query(
        "SELECT octet_length(payload) AS bytes FROM entity_snapshots WHERE object_id=ANY($1)",
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut total = 0usize;
    for row in sizes {
        let bytes = usize::try_from(row.get::<i32, _>("bytes"))
            .map_err(|_| StoreError::Invalid("construction prior bytes"))?;
        if bytes > 2 * 1024 * 1024 + 52 {
            return Err(StoreError::Invalid("construction prior bytes"));
        }
        total = total
            .checked_add(bytes)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or(StoreError::Invalid("construction prior batch bytes"))?;
    }
    let rows =
        sqlx::query("SELECT object_id,payload FROM entity_snapshots WHERE object_id=ANY($1)")
            .bind(ids)
            .fetch_all(&mut **tx)
            .await?;
    for row in rows {
        let id = row.get::<i64, _>("object_id") as u32;
        let snapshot = snapshots
            .get(&id)
            .ok_or(StoreError::Invalid("missing construction snapshot"))?;
        let prior: Vec<u8> = row.get("payload");
        let header = bace_storage_codec::inspect(
            &prior,
            bace_storage_codec::CodecLimits {
                max_payload_bytes: 2 * 1024 * 1024,
            },
        )
        .map_err(|_| StoreError::Invalid("prior item identity envelope"))?;
        let after = ItemSaveV5::decode_or_migrate(&snapshot.bytes, None)
            .map_err(|_| StoreError::Invalid("next item identity payload"))?;
        if header.schema_version == 1 {
            if after.source_destination.is_some() || after.construction.is_some() {
                return Err(StoreError::Invalid("legacy item origin cannot be inferred"));
            }
            continue;
        }
        let before = ItemSaveV5::decode_or_migrate(&prior, None)
            .map_err(|_| StoreError::Invalid("prior item identity payload"))?;
        bace_storage_codec::validate_item_source_destination_transition(&before, &after)
            .map_err(|_| StoreError::Invalid("item construction or source origin changed"))?;
    }
    Ok(())
}
