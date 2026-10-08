use crate::{PgStore, StoreError};
use bace_persistence::{
    ContentCandidate, MappedContentCandidate, NativeContentCandidate, NativePublication,
};
use sqlx::Row;

impl PgStore {
    /// One reviewed inbox batch enters the same commit-ordered journal as SQL
    /// authoring. An uncertain COMMIT must be resolved from the journal before
    /// retrying; callers must not assume a failed acknowledgment means rollback.
    pub async fn insert_mapped_batch(
        &self,
        weenies: &[ContentCandidate],
        native: &[NativeContentCandidate],
        mapped: &[MappedContentCandidate],
        expected_manifest_hash: [u8; 32],
    ) -> Result<i64, StoreError> {
        let count = weenies.len() + native.len() + mapped.len();
        let bytes = weenies
            .iter()
            .map(|c| c.bytes.len())
            .chain(native.iter().map(|c| c.bytes.len()))
            .chain(mapped.iter().map(|c| c.bytes.as_ref().map_or(0, Vec::len)))
            .try_fold(0usize, |total, len| total.checked_add(len));
        if count == 0 || count > 4096 || bytes.is_none_or(|n| n > 16 * 1024 * 1024) {
            return Err(StoreError::Invalid("mapped batch capacity"));
        }
        let mut keys = std::collections::BTreeSet::new();
        for c in weenies {
            if c.wcid == 0 || c.bytes.is_empty() || !keys.insert((1, u64::from(c.wcid))) {
                return Err(StoreError::Invalid("duplicate or invalid weenie"));
            }
        }
        for c in native {
            if !matches!(c.namespace, 46 | 47)
                || c.id == 0
                || c.schema != 1
                || c.bytes.is_empty()
                || !keys.insert((c.namespace, u64::from(c.id)))
            {
                return Err(StoreError::Invalid("duplicate or invalid native profile"));
            }
        }
        for c in mapped {
            if !matches!(c.namespace, 1 | 16..=47 | 50)
                || c.id > u64::from(u32::MAX)
                || c.schema != 1
                || c.bytes.as_ref().is_some_and(Vec::is_empty)
                || c.bytes
                    .as_ref()
                    .is_some_and(|_| !matches!(c.namespace, 16..=45 | 50))
                || !keys.insert((c.namespace, c.id))
            {
                return Err(StoreError::Invalid("duplicate or invalid mapped record"));
            }
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT revision FROM content_clock WHERE singleton FOR UPDATE")
            .execute(&mut *tx)
            .await?;
        let active: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT active_manifest_hash FROM content_runtime_state WHERE singleton FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        let pending: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_publications WHERE status='pending')",
        )
        .fetch_one(&mut *tx)
        .await?;
        if active.as_deref() != Some(expected_manifest_hash.as_slice()) {
            return Err(StoreError::GenerationConflict);
        }
        if pending {
            return Err(StoreError::Invalid(
                "wait for the pending content publication before queueing another inbox batch",
            ));
        }
        let mut revision = 0;
        for c in weenies {
            revision = sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES($1,$2,$3,$4) RETURNING revision")
                .bind(i64::from(c.wcid)).bind(&c.class_name).bind(c.weenie_type).bind(&c.bytes)
                .fetch_one(&mut *tx).await?;
        }
        for c in native {
            revision = sqlx::query_scalar("INSERT INTO native_content_candidates(namespace,content_id,schema_version,payload) VALUES($1,$2,$3,$4) RETURNING revision")
                .bind(i32::from(c.namespace)).bind(i64::from(c.id)).bind(i32::from(c.schema)).bind(&c.bytes)
                .fetch_one(&mut *tx).await?;
        }
        for c in mapped {
            revision = sqlx::query_scalar("INSERT INTO mapped_content_candidates(namespace,content_id,schema_version,payload) VALUES($1,$2,$3,$4) RETURNING revision")
                .bind(i32::from(c.namespace)).bind(c.id as i64).bind(i32::from(c.schema)).bind(&c.bytes)
                .fetch_one(&mut *tx).await?;
        }
        sqlx::query("UPDATE content_publications SET expected_manifest_hash=$2 WHERE revision=$1")
            .bind(revision)
            .bind(expected_manifest_hash.as_slice())
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(revision)
    }
    /// Journal an immutable bounded batch. This does not validate or activate it.
    pub async fn insert_native_candidates(
        &self,
        candidates: &[NativeContentCandidate],
    ) -> Result<i64, StoreError> {
        let bytes = candidates
            .iter()
            .try_fold(0usize, |sum, c| sum.checked_add(c.bytes.len()));
        if candidates.is_empty()
            || candidates.len() > 4096
            || bytes.is_none_or(|n| n > 16 * 1024 * 1024)
            || candidates.iter().any(|c| {
                !matches!(c.namespace, 46 | 47) || c.id == 0 || c.schema == 0 || c.bytes.is_empty()
            })
        {
            return Err(StoreError::Invalid("native candidate limits or identity"));
        }
        let mut tx = self.pool.begin().await?;
        let mut revision = 0;
        for c in candidates {
            revision=sqlx::query_scalar("INSERT INTO native_content_candidates(namespace,content_id,schema_version,payload) VALUES($1,$2,$3,$4) RETURNING revision")
                .bind(i32::from(c.namespace)).bind(i64::from(c.id)).bind(i32::from(c.schema)).bind(&c.bytes).fetch_one(&mut *tx).await?;
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(revision)
    }

    /// Read exactly the oldest transaction. Larger SQL-authored transactions
    /// remain pending and return an explicit capacity error; never a partial batch.
    pub async fn pending_native_publication(
        &self,
    ) -> Result<Option<NativePublication>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let row=sqlx::query("SELECT revision,candidate_count,payload_bytes,expected_manifest_hash FROM content_publications WHERE status='pending' ORDER BY revision LIMIT 1").fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let revision: i64 = row.get("revision");
        let expected_manifest_hash: Option<[u8; 32]> = row
            .get::<Option<Vec<u8>>, _>("expected_manifest_hash")
            .map(|bytes| {
                bytes
                    .try_into()
                    .map_err(|_| StoreError::Invalid("expected manifest hash"))
            })
            .transpose()?;
        if row.get::<i32, _>("candidate_count") > 4096
            || row.get::<i64, _>("payload_bytes") > 16 * 1024 * 1024
        {
            return Err(StoreError::PublicationTooLarge(revision));
        }
        let weenies: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM content_candidates WHERE revision=$1")
                .bind(revision)
                .fetch_one(&mut *tx)
                .await?;
        let rows=sqlx::query("SELECT namespace,content_id,schema_version,payload FROM native_content_candidates WHERE revision=$1 ORDER BY namespace,content_id").bind(revision).fetch_all(&mut *tx).await?;
        let candidates = rows
            .into_iter()
            .map(|r| NativeContentCandidate {
                namespace: r.get::<i32, _>("namespace") as u16,
                id: r.get::<i64, _>("content_id") as u32,
                schema: r.get::<i32, _>("schema_version") as u16,
                bytes: r.get("payload"),
            })
            .collect();
        let mapped_rows=sqlx::query("SELECT namespace,content_id,schema_version,payload FROM mapped_content_candidates WHERE revision=$1 ORDER BY namespace,content_id").bind(revision).fetch_all(&mut *tx).await?;
        let mapped_candidates = mapped_rows
            .into_iter()
            .map(|r| MappedContentCandidate {
                namespace: r.get::<i32, _>("namespace") as u16,
                id: r.get::<i64, _>("content_id") as u64,
                schema: r.get::<i32, _>("schema_version") as u16,
                bytes: r.get("payload"),
            })
            .collect();
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(Some(NativePublication {
            revision,
            expected_manifest_hash,
            candidates,
            mapped_candidates,
            weenie_candidates: weenies as usize,
        }))
    }

    pub async fn native_content_revision(
        &self,
        namespace: u16,
        id: u32,
    ) -> Result<Option<i64>, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT revision FROM native_content_heads WHERE namespace=$1 AND content_id=$2",
        )
        .bind(i32::from(namespace))
        .bind(i64::from(id))
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn mapped_content_revision(
        &self,
        namespace: u16,
        id: u32,
    ) -> Result<Option<i64>, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT revision FROM mapped_content_heads WHERE namespace=$1 AND content_id=$2",
        )
        .bind(i32::from(namespace))
        .bind(i64::from(id))
        .fetch_optional(&self.pool)
        .await?)
    }
}
