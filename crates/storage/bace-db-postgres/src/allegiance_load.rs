//! One bounded repeatable snapshot for cold forest restoration. The payload
//! budget is checked before allocating rows, including metadata in the same budget.
use crate::{PgStore, StoreError};
use bace_persistence::StoredAllegiance;
use sqlx::Row;
impl PgStore {
    pub async fn load_allegiance_forest(
        &self,
        maximum_nodes: usize,
        maximum_bytes: usize,
    ) -> Result<(Vec<StoredAllegiance>, Vec<StoredAllegiance>), StoreError> {
        if maximum_nodes == 0
            || maximum_nodes > 65536
            || maximum_bytes == 0
            || maximum_bytes > 64 * 1024 * 1024
        {
            return Err(StoreError::Invalid("allegiance forest budget"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let (nodes, metadata, bytes): (i64, i64, i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM allegiance_nodes),(SELECT COUNT(*) FROM allegiance_metadata),((SELECT COALESCE(SUM(octet_length(payload)),0) FROM allegiance_nodes)+(SELECT COALESCE(SUM(octet_length(payload)),0) FROM allegiance_metadata))::bigint")
            .fetch_one(&mut *tx).await?;
        if nodes > maximum_nodes as i64 || metadata > nodes || bytes > maximum_bytes as i64 {
            return Err(StoreError::Invalid(
                "allegiance forest exceeds startup capacity",
            ));
        }
        let node_rows = sqlx::query("SELECT character_id AS id,version,mutation_revision,payload FROM allegiance_nodes ORDER BY character_id").fetch_all(&mut *tx).await?;
        let metadata_rows = sqlx::query("SELECT monarch_id AS id,version,mutation_revision,payload FROM allegiance_metadata ORDER BY monarch_id").fetch_all(&mut *tx).await?;
        let convert = |rows: Vec<sqlx::postgres::PgRow>| {
            rows.into_iter()
                .map(|r| {
                    Ok(StoredAllegiance {
                        character: u32::try_from(r.get::<i64, _>("id"))
                            .map_err(|_| StoreError::Invalid("allegiance identity"))?,
                        mutation_revision: u64::try_from(r.get::<i64, _>("mutation_revision"))
                            .map_err(|_| StoreError::Invalid("allegiance revision"))?,
                        persisted_version: r.get("version"),
                        bytes: r.get("payload"),
                    })
                })
                .collect::<Result<Vec<_>, StoreError>>()
        };
        let result = (convert(node_rows)?, convert(metadata_rows)?);
        tx.rollback().await?;
        Ok(result)
    }
}

impl PgStore {
    pub async fn allegiance_placement_operation(
        &self,
        op: &bace_persistence::AllegiancePlacementOperation,
    ) -> Result<bace_persistence::OperationOutcome, StoreError> {
        if op.world_epoch == 0
            || op.world_epoch > i64::MAX as u64
            || op
                .workflow
                .as_ref()
                .is_some_and(|w| w.world_epoch != op.world_epoch)
        {
            return Err(StoreError::Invalid("allegiance placement epoch"));
        }
        crate::placements::execute(
            self,
            &op.placement,
            None,
            op.workflow.as_ref(),
            op.workflow.is_none().then_some(op.world_epoch),
            Some(&op.allegiance),
        )
        .await
    }
}
