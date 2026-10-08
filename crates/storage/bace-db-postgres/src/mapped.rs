use crate::{PgStore, StoreError};
use bace_persistence::{ContentCandidate, MappedGeneration};
use sha2::{Digest, Sha256};
use sqlx::Row;

impl PgStore {
    /// Caller has validated and durably installed every referenced file and the manifest.
    /// None installs an initial/repacked layout without changing logical accepted content.
    pub async fn accept_mapped(
        &self,
        revision: Option<i64>,
        generation: &MappedGeneration,
    ) -> Result<(), StoreError> {
        if revision.is_some_and(|r| r != generation.accepted_revision)
            || generation.manifest_bytes.is_empty()
            || generation.manifest_bytes.len() > 1024 * 1024
            || generation.accepted_revision < 0
            || Sha256::digest(&generation.manifest_bytes).as_slice() != generation.manifest_hash
        {
            return Err(StoreError::Invalid(
                "mapped manifest hash, revision or size",
            ));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(42812001)")
            .execute(&mut *tx)
            .await?;
        let active: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT active_manifest_hash FROM content_runtime_state WHERE singleton FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        if active.as_deref() == Some(generation.manifest_hash.as_slice()) {
            let existing=sqlx::query("SELECT parent_hash,base_hash,accepted_revision,manifest_bytes FROM content_generations WHERE manifest_hash=$1").bind(generation.manifest_hash.as_slice()).fetch_one(&mut *tx).await?;
            if existing.get::<Option<Vec<u8>>, _>("parent_hash").as_deref()
                != generation.parent_hash.as_ref().map(|h| h.as_slice())
                || existing.get::<Vec<u8>, _>("base_hash") != generation.base_hash
                || existing.get::<i64, _>("accepted_revision") != generation.accepted_revision
                || existing.get::<Vec<u8>, _>("manifest_bytes") != generation.manifest_bytes
            {
                return Err(StoreError::OperationMismatch);
            }
            return Ok(());
        }
        if active.as_deref() != generation.parent_hash.as_ref().map(|h| h.as_slice()) {
            return Err(StoreError::GenerationConflict);
        }
        let accepted: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision),0) FROM content_publications WHERE status='accepted'",
        )
        .fetch_one(&mut *tx)
        .await?;
        if let Some(revision) = revision {
            let first:Option<i64>=sqlx::query_scalar("SELECT revision FROM content_publications WHERE status='pending' ORDER BY revision LIMIT 1").fetch_optional(&mut *tx).await?;
            if first != Some(revision)
                || generation.accepted_revision != revision
                || revision <= accepted
            {
                return Err(StoreError::PublicationOrder);
            }
            // Remove only this batch's prior heads inside the same transaction,
            // so valid class-name swaps do not conflict with their own old rows.
            sqlx::query("DELETE FROM content_heads WHERE wcid IN (SELECT wcid FROM content_candidates WHERE revision=$1)").bind(revision).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO content_heads(wcid,revision,class_name) SELECT wcid,revision,class_name FROM content_candidates WHERE revision=$1").bind(revision).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO native_content_heads(namespace,content_id,revision) SELECT namespace,content_id,revision FROM native_content_candidates WHERE revision=$1 ON CONFLICT(namespace,content_id) DO UPDATE SET revision=EXCLUDED.revision").bind(revision).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM content_heads WHERE wcid IN (SELECT content_id FROM mapped_content_candidates WHERE revision=$1 AND namespace=1 AND payload IS NULL)").bind(revision).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM native_content_heads WHERE (namespace,content_id) IN (SELECT namespace,content_id FROM mapped_content_candidates WHERE revision=$1 AND namespace IN (46,47) AND payload IS NULL)").bind(revision).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM mapped_content_heads WHERE (namespace,content_id) IN (SELECT namespace,content_id FROM mapped_content_candidates WHERE revision=$1)").bind(revision).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO mapped_content_heads(namespace,content_id,revision) SELECT namespace,content_id,revision FROM mapped_content_candidates WHERE revision=$1 AND payload IS NOT NULL").bind(revision).execute(&mut *tx).await?;
            sqlx::query("UPDATE content_publications SET status='accepted' WHERE revision=$1")
                .bind(revision)
                .execute(&mut *tx)
                .await?;
        } else if generation.accepted_revision != accepted {
            return Err(StoreError::GenerationConflict);
        }
        sqlx::query("INSERT INTO content_generations(manifest_hash,parent_hash,base_hash,accepted_revision,manifest_bytes) VALUES($1,$2,$3,$4,$5)")
            .bind(generation.manifest_hash.as_slice()).bind(generation.parent_hash.as_ref().map(|h|h.as_slice())).bind(generation.base_hash.as_slice()).bind(generation.accepted_revision).bind(&generation.manifest_bytes).execute(&mut *tx).await?;
        sqlx::query("UPDATE content_runtime_state SET active_manifest_hash=$1 WHERE singleton")
            .bind(generation.manifest_hash.as_slice())
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)
    }
    pub async fn active_generation(&self) -> Result<Option<MappedGeneration>, StoreError> {
        let row=sqlx::query("SELECT g.* FROM content_runtime_state s JOIN content_generations g ON g.manifest_hash=s.active_manifest_hash WHERE singleton").fetch_optional(&self.pool).await?;
        row.map(|r| {
            Ok(MappedGeneration {
                manifest_hash: r
                    .get::<Vec<u8>, _>("manifest_hash")
                    .try_into()
                    .map_err(|_| StoreError::Invalid("manifest hash"))?,
                parent_hash: r
                    .get::<Option<Vec<u8>>, _>("parent_hash")
                    .map(|h| h.try_into().map_err(|_| StoreError::Invalid("parent hash")))
                    .transpose()?,
                base_hash: r
                    .get::<Vec<u8>, _>("base_hash")
                    .try_into()
                    .map_err(|_| StoreError::Invalid("base hash"))?,
                accepted_revision: r.get("accepted_revision"),
                manifest_bytes: r.get("manifest_bytes"),
            })
        })
        .transpose()
    }
    /// Durable continuations retain their exact immutable accepted generation,
    /// even after a newer head is published. The caller still verifies pack files.
    pub async fn generation_by_hash(
        &self,
        hash: [u8; 32],
    ) -> Result<Option<MappedGeneration>, StoreError> {
        let row = sqlx::query("SELECT parent_hash,base_hash,accepted_revision,manifest_bytes FROM content_generations WHERE manifest_hash=$1")
            .bind(hash.as_slice()).fetch_optional(&self.pool).await?;
        row.map(|row| {
            let bytes: Vec<u8> = row.get("manifest_bytes");
            if bytes.len() > 1024 * 1024 || Sha256::digest(&bytes).as_slice() != hash {
                return Err(StoreError::Invalid("historical manifest integrity"));
            }
            Ok(MappedGeneration {
                manifest_hash: hash,
                parent_hash: row
                    .get::<Option<Vec<u8>>, _>("parent_hash")
                    .map(|h| {
                        h.try_into()
                            .map_err(|_| StoreError::Invalid("historical parent hash"))
                    })
                    .transpose()?,
                base_hash: row
                    .get::<Vec<u8>, _>("base_hash")
                    .try_into()
                    .map_err(|_| StoreError::Invalid("historical base hash"))?,
                accepted_revision: row.get("accepted_revision"),
                manifest_bytes: bytes,
            })
        })
        .transpose()
    }
    pub async fn pending_revision(&self) -> Result<Option<i64>, StoreError> {
        Ok(sqlx::query_scalar("SELECT revision FROM content_publications WHERE status='pending' ORDER BY revision LIMIT 1").fetch_optional(&self.pool).await?)
    }
    /// At most 16 records / 272 MiB; use limit=1 for a single-record memory bound.
    pub async fn candidate_page(
        &self,
        revision: i64,
        after_wcid: u32,
        limit: u32,
    ) -> Result<Vec<ContentCandidate>, StoreError> {
        if !(1..=16).contains(&limit) {
            return Err(StoreError::Invalid("candidate page limit"));
        }
        let rows=sqlx::query("SELECT wcid,class_name,weenie_type,payload FROM content_candidates WHERE revision=$1 AND wcid>$2 ORDER BY wcid LIMIT $3").bind(revision).bind(i64::from(after_wcid)).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| ContentCandidate {
                wcid: r.get::<i64, _>("wcid") as u32,
                class_name: r.get("class_name"),
                weenie_type: r.get("weenie_type"),
                bytes: r.get("payload"),
            })
            .collect())
    }
}
