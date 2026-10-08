//! Indexed allegiance persistence with CAS, bounded lineage validation and
//! idempotent joint player-XP commits. All SQL remains in this crate.
use crate::{PgStore, StoreError};
use bace_persistence::{
    AllegianceCommit, AllegianceOperation, AllegianceWrite, OwnershipState, SaveAck,
    StoredAllegiance,
};
use bace_storage_codec::{AllegianceMetadataV1, AllegianceNodeV1};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};

impl PgStore {
    pub async fn load_allegiance_nodes(
        &self,
        ids: &[u32],
    ) -> Result<Vec<StoredAllegiance>, StoreError> {
        load(self, ids, false).await
    }
    pub async fn load_allegiance_metadata(
        &self,
        ids: &[u32],
    ) -> Result<Vec<StoredAllegiance>, StoreError> {
        load(self, ids, true).await
    }
    pub async fn allegiance_operation(
        &self,
        op: &AllegianceOperation,
    ) -> Result<AllegianceCommit, StoreError> {
        validate(op)?;
        let hash = fingerprint(op);
        let mut tx = self.pool.begin().await?;
        let inserted=sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(&op.operation_id).bind(hash.as_slice()).execute(&mut *tx).await?.rows_affected();
        if inserted == 0 {
            let old: Vec<u8> = sqlx::query_scalar(
                "SELECT request_fingerprint FROM durable_operations WHERE operation_id=$1",
            )
            .bind(&op.operation_id)
            .fetch_one(&mut *tx)
            .await?;
            if old != hash {
                return Err(StoreError::OperationMismatch);
            }
            tx.commit().await.map_err(crate::store::commit_error)?;
            return Ok(AllegianceCommit::AlreadyCommitted);
        }
        // Match the hierarchy lock order used by owned player writes. The
        // allegiance sequencer serializes relationship edits, not simulation.
        sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(8589934589)")
            .execute(&mut *tx)
            .await?;
        let ids: BTreeSet<_> = op.leases.iter().map(|l| l.character_id).collect();
        for id in &ids {
            crate::ownership::lock_object(&mut tx, *id).await?;
        }
        for lease in &op.leases {
            crate::ownership::check_lease(&mut tx, *lease).await?;
        }
        let (node_acks, metadata_acks) = apply_patch(&mut tx, op).await?;
        let players = crate::writes::write_all_unchecked(&mut tx, &op.players).await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(AllegianceCommit::Committed {
            nodes: node_acks,
            metadata: metadata_acks,
            players,
        })
    }
}
pub(crate) fn validate(op: &AllegianceOperation) -> Result<(), StoreError> {
    validate_ledger(op, false)
}
pub(crate) fn validate_ledger(
    op: &AllegianceOperation,
    allow_empty: bool,
) -> Result<(), StoreError> {
    if op.operation_id.is_empty()
        || op.operation_id.len() > 128
        || op.nodes.len() + op.metadata.len() > 1024
        || !allow_empty && op.nodes.is_empty() && op.metadata.is_empty() && op.players.is_empty()
        || op.leases.len() > 1024
    {
        return Err(StoreError::Invalid("allegiance operation bounds"));
    }
    let mut bytes = 0usize;
    for (metadata, rows) in [(false, &op.nodes), (true, &op.metadata)] {
        let mut ids = BTreeSet::new();
        for write in rows {
            if write.character == 0
                || !ids.insert(write.character)
                || write.expected_version < 0
                || write.expected_version == i64::MAX
                || write.mutation_revision == 0
                || write.mutation_revision > i64::MAX as u64
                || write.bytes.is_none() && write.expected_version == 0
            {
                return Err(StoreError::Invalid("allegiance write identity"));
            }
            if let Some(payload) = &write.bytes {
                bytes = bytes
                    .checked_add(payload.len())
                    .ok_or(StoreError::Invalid("allegiance bytes"))?;
                let identity = if metadata {
                    AllegianceMetadataV1::decode(payload).map(|n| n.monarch)
                } else {
                    AllegianceNodeV1::decode(payload).map(|n| n.character)
                }
                .map_err(|_| StoreError::Invalid("allegiance payload"))?;
                if identity != write.character {
                    return Err(StoreError::Invalid("allegiance embedded identity"));
                }
            }
        }
    }
    if !op.players.is_empty() {
        crate::writes::validate(&op.players)?;
    }
    bytes = bytes
        .checked_add(op.players.iter().map(|p| p.bytes.len()).sum())
        .ok_or(StoreError::Invalid("allegiance bytes"))?;
    if bytes > 64 * 1024 * 1024 {
        return Err(StoreError::Invalid("allegiance bytes"));
    }
    let mut leases = BTreeSet::new();
    for lease in &op.leases {
        if !leases.insert(lease.character_id)
            || lease.epoch < 0
            || !matches!(
                lease.state,
                OwnershipState::Online | OwnershipState::Offline
            )
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    if op.players.iter().any(|p| !leases.contains(&p.object_id))
        || op.nodes.iter().any(|p| !leases.contains(&p.character))
        || op.metadata.iter().any(|p| !leases.contains(&p.character))
    {
        return Err(StoreError::OwnershipConflict);
    }
    Ok(())
}
async fn write_node(
    tx: &mut Transaction<'_, Postgres>,
    w: &AllegianceWrite,
    node: Option<&AllegianceNodeV1>,
) -> Result<SaveAck, StoreError> {
    let n = if let Some(n) = node {
        if w.expected_version == 0 {
            sqlx::query("INSERT INTO allegiance_nodes(character_id,account_id,patron_id,monarch_id,version,mutation_revision,payload) VALUES($1,$2,$3,$4,1,$5,$6) ON CONFLICT DO NOTHING")
                .bind(i64::from(w.character)).bind(i64::try_from(n.account).map_err(|_|StoreError::Invalid("account range"))?).bind(n.patron.map(i64::from)).bind(i64::from(n.monarch)).bind(w.mutation_revision as i64).bind(w.bytes.as_ref().unwrap()).execute(&mut **tx).await?.rows_affected()
        } else {
            sqlx::query("UPDATE allegiance_nodes SET patron_id=$3,monarch_id=$4,version=version+1,mutation_revision=$5,payload=$6 WHERE character_id=$1 AND version=$2 AND mutation_revision<$5 AND account_id=$7")
                .bind(i64::from(w.character)).bind(w.expected_version).bind(n.patron.map(i64::from)).bind(i64::from(n.monarch)).bind(w.mutation_revision as i64).bind(w.bytes.as_ref().unwrap()).bind(i64::try_from(n.account).map_err(|_|StoreError::Invalid("account range"))?).execute(&mut **tx).await?.rows_affected()
        }
    } else {
        sqlx::query("DELETE FROM allegiance_nodes WHERE character_id=$1 AND version=$2 AND mutation_revision<$3").bind(i64::from(w.character)).bind(w.expected_version).bind(w.mutation_revision as i64).execute(&mut **tx).await?.rows_affected()
    };
    ack(w, n)
}
async fn write_metadata(
    tx: &mut Transaction<'_, Postgres>,
    w: &AllegianceWrite,
) -> Result<SaveAck, StoreError> {
    let n = if let Some(bytes) = &w.bytes {
        let metadata = AllegianceMetadataV1::decode(bytes)
            .map_err(|_| StoreError::Invalid("allegiance metadata"))?;
        if w.expected_version == 0 {
            sqlx::query("INSERT INTO allegiance_metadata(monarch_id,version,mutation_revision,payload,chat_room) VALUES($1,1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(i64::from(w.character)).bind(w.mutation_revision as i64).bind(bytes).bind(i64::from(metadata.chat_room)).execute(&mut **tx).await?.rows_affected()
        } else {
            sqlx::query("UPDATE allegiance_metadata SET version=version+1,mutation_revision=$3,payload=$4,chat_room=$5 WHERE monarch_id=$1 AND version=$2 AND mutation_revision<$3").bind(i64::from(w.character)).bind(w.expected_version).bind(w.mutation_revision as i64).bind(bytes).bind(i64::from(metadata.chat_room)).execute(&mut **tx).await?.rows_affected()
        }
    } else {
        sqlx::query("DELETE FROM allegiance_metadata WHERE monarch_id=$1 AND version=$2 AND mutation_revision<$3").bind(i64::from(w.character)).bind(w.expected_version).bind(w.mutation_revision as i64).execute(&mut **tx).await?.rows_affected()
    };
    ack(w, n)
}
fn ack(w: &AllegianceWrite, n: u64) -> Result<SaveAck, StoreError> {
    if n != 1 {
        return Err(StoreError::Conflict(w.character));
    }
    Ok(SaveAck {
        object_id: w.character,
        mutation_revision: w.mutation_revision,
        persisted_version: w.expected_version + 1,
    })
}
async fn validate_edges(
    tx: &mut Transaction<'_, Postgres>,
    changed: &BTreeMap<u32, Option<AllegianceNodeV1>>,
    old_parents: &[i64],
) -> Result<(), StoreError> {
    let ids: Vec<_> = changed.keys().map(|id| i64::from(*id)).collect();
    let bad:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM allegiance_nodes n LEFT JOIN allegiance_nodes p ON p.character_id=n.patron_id WHERE (n.character_id=ANY($1) OR n.patron_id=ANY($1)) AND (n.patron_id IS NOT NULL AND (p.character_id IS NULL OR n.monarch_id<>p.monarch_id)))").bind(&ids).fetch_one(&mut **tx).await?;
    if bad {
        return Err(StoreError::Invalid("allegiance monarch edge mismatch"));
    }
    for (id, node) in changed {
        let mut children:Vec<i64>=sqlx::query_scalar("SELECT character_id FROM allegiance_nodes WHERE patron_id=$1 ORDER BY character_id LIMIT 12").bind(i64::from(*id)).fetch_all(&mut **tx).await?;
        if let Some(node) = node {
            let mut expected: Vec<_> = node.vassals.iter().copied().map(i64::from).collect();
            expected.sort_unstable();
            children.sort_unstable();
            if children != expected {
                return Err(StoreError::Invalid("allegiance vassal edges"));
            }
            let roots:Vec<(i64,Option<i64>,i32)>=sqlx::query_as("WITH RECURSIVE lineage AS (SELECT character_id,patron_id,0 AS depth FROM allegiance_nodes WHERE character_id=$1 UNION ALL SELECT n.character_id,n.patron_id,l.depth+1 FROM allegiance_nodes n JOIN lineage l ON n.character_id=l.patron_id WHERE l.depth<1024) SELECT character_id,patron_id,depth FROM lineage WHERE patron_id IS NULL OR depth=1024")
                .bind(i64::from(*id)).fetch_all(&mut **tx).await?;
            if roots.len() != 1 || roots[0].1.is_some() || roots[0].0 != i64::from(node.monarch) {
                return Err(StoreError::Invalid("allegiance cycle or depth"));
            }
        } else if !children.is_empty() {
            return Err(StoreError::Invalid(
                "removed allegiance patron retains children",
            ));
        }
    }
    // Payloads of unchanged affected patrons must agree with the new edges too.
    let parents:Vec<(i64,Vec<u8>)>=sqlx::query_as("SELECT DISTINCT p.character_id,p.payload FROM allegiance_nodes p WHERE p.character_id=ANY($2) OR p.character_id IN (SELECT patron_id FROM allegiance_nodes WHERE character_id=ANY($1))").bind(&ids).bind(old_parents).fetch_all(&mut **tx).await?;
    for (id, bytes) in parents {
        let parent = AllegianceNodeV1::decode(&bytes)
            .map_err(|_| StoreError::Invalid("stored allegiance parent"))?;
        let actual:Vec<i64>=sqlx::query_scalar("SELECT character_id FROM allegiance_nodes WHERE patron_id=$1 ORDER BY character_id LIMIT 12").bind(id).fetch_all(&mut **tx).await?;
        let mut expected: Vec<_> = parent.vassals.into_iter().map(i64::from).collect();
        expected.sort_unstable();
        if actual != expected {
            return Err(StoreError::Invalid("unchanged allegiance patron mismatch"));
        }
    }
    Ok(())
}
async fn load(
    store: &PgStore,
    ids: &[u32],
    metadata: bool,
) -> Result<Vec<StoredAllegiance>, StoreError> {
    if ids.len() > 1024 {
        return Err(StoreError::Invalid("allegiance read count"));
    }
    let ids: Vec<i64> = ids.iter().copied().map(i64::from).collect();
    let query = if metadata {
        "SELECT monarch_id AS id,version,mutation_revision,payload FROM allegiance_metadata WHERE monarch_id=ANY($1) ORDER BY monarch_id"
    } else {
        "SELECT character_id AS id,version,mutation_revision,payload FROM allegiance_nodes WHERE character_id=ANY($1) ORDER BY character_id"
    };
    let mut tx = store.pool.begin().await?;
    // A repeatable snapshot bounds total bytes before fetching any payload.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    let count_query = if metadata {
        "SELECT COALESCE(SUM(octet_length(payload)),0)::bigint FROM allegiance_metadata WHERE monarch_id=ANY($1)"
    } else {
        "SELECT COALESCE(SUM(octet_length(payload)),0)::bigint FROM allegiance_nodes WHERE character_id=ANY($1)"
    };
    let size: i64 = sqlx::query_scalar(count_query)
        .bind(&ids)
        .fetch_one(&mut *tx)
        .await?;
    if size > 64 * 1024 * 1024 {
        return Err(StoreError::Invalid("allegiance read bytes"));
    }
    let rows = sqlx::query(query).bind(&ids).fetch_all(&mut *tx).await?;
    tx.rollback().await?;
    Ok(rows
        .into_iter()
        .map(|r| StoredAllegiance {
            character: r.get::<i64, _>("id") as u32,
            mutation_revision: r.get::<i64, _>("mutation_revision") as u64,
            persisted_version: r.get("version"),
            bytes: r.get("payload"),
        })
        .collect())
}
pub(crate) fn fingerprint(op: &AllegianceOperation) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"betterace-allegiance-v1");
    for rows in [&op.nodes, &op.metadata] {
        hash.update((rows.len() as u64).to_le_bytes());
        for w in rows {
            hash.update(w.character.to_le_bytes());
            hash.update(w.mutation_revision.to_le_bytes());
            hash.update(w.expected_version.to_le_bytes());
            hash.update([u8::from(w.bytes.is_some())]);
            if let Some(bytes) = &w.bytes {
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
            }
        }
    }
    hash.update((op.players.len() as u64).to_le_bytes());
    for p in &op.players {
        hash.update(p.object_id.to_le_bytes());
        hash.update(p.mutation_revision.to_le_bytes());
        hash.update(p.expected_version.to_le_bytes());
        hash.update((p.bytes.len() as u64).to_le_bytes());
        hash.update(&p.bytes);
    }
    hash.update((op.leases.len() as u64).to_le_bytes());
    for l in &op.leases {
        hash.update(l.character_id.to_le_bytes());
        hash.update(l.epoch.to_le_bytes());
        hash.update([u8::from(l.state == OwnershipState::Online)]);
    }
    hash.finalize().into()
}

pub(crate) async fn apply_patch(
    tx: &mut Transaction<'_, Postgres>,
    op: &AllegianceOperation,
) -> Result<(Vec<SaveAck>, Vec<SaveAck>), StoreError> {
    let changed_ids: Vec<i64> = op.nodes.iter().map(|n| i64::from(n.character)).collect();
    let old_parents:Vec<i64>=sqlx::query_scalar("SELECT DISTINCT patron_id FROM allegiance_nodes WHERE character_id=ANY($1) AND patron_id IS NOT NULL").bind(&changed_ids).fetch_all(&mut **tx).await?;
    let mut node_acks = Vec::with_capacity(op.nodes.len());
    let mut changed = BTreeMap::new();
    for write in &op.nodes {
        let value = write
            .bytes
            .as_deref()
            .map(AllegianceNodeV1::decode)
            .transpose()
            .map_err(|_| StoreError::Invalid("allegiance node payload"))?;
        if let Some(node) = &value {
            let identity: Option<(i64, String)> =
                sqlx::query_as("SELECT account_id,name FROM players WHERE object_id=$1")
                    .bind(i64::from(node.character))
                    .fetch_optional(&mut **tx)
                    .await?;
            if identity
                != Some((
                    i64::try_from(node.account)
                        .map_err(|_| StoreError::Invalid("account range"))?,
                    node.name.clone(),
                ))
            {
                return Err(StoreError::OwnershipConflict);
            }
        }
        node_acks.push(write_node(tx, write, value.as_ref()).await?);
        changed.insert(write.character, value);
    }
    let mut metadata_acks = Vec::with_capacity(op.metadata.len());
    for write in &op.metadata {
        metadata_acks.push(write_metadata(tx, write).await?);
    }
    validate_edges(tx, &changed, &old_parents).await?;
    let metadata_ids: Vec<i64> = op.metadata.iter().map(|m| i64::from(m.character)).collect();
    let invalid_metadata:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM allegiance_metadata m JOIN allegiance_nodes n ON n.character_id=m.monarch_id WHERE (m.monarch_id=ANY($1) OR m.monarch_id=ANY($2)) AND n.patron_id IS NOT NULL)").bind(&changed_ids).bind(&metadata_ids).fetch_one(&mut **tx).await?;
    if invalid_metadata {
        return Err(StoreError::Invalid("allegiance metadata requires monarch"));
    }
    Ok((node_acks, metadata_acks))
}
