use crate::{PgStore, StoreError};
use bace_persistence::{ContentCandidate, Publication};
use sqlx::Row;
/// Scalar journal statistics from one database snapshot; no content payloads are read.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContentStatus {
    pub accepted_revision: i64,
    pub active_templates: i64,
    pub pending_publications: i64,
    pub rejected_publications: i64,
}
impl PgStore {
    pub async fn content_status(&self) -> Result<ContentStatus, StoreError> {
        let row = sqlx::query("SELECT COALESCE(MAX(revision) FILTER (WHERE status='accepted'),0) AS accepted_revision, COUNT(*) FILTER (WHERE status='pending') AS pending_publications, COUNT(*) FILTER (WHERE status='rejected') AS rejected_publications, (SELECT COUNT(*) FROM content_heads) AS active_templates FROM content_publications")
            .fetch_one(&self.pool).await?;
        Ok(ContentStatus {
            accepted_revision: row.get("accepted_revision"),
            active_templates: row.get("active_templates"),
            pending_publications: row.get("pending_publications"),
            rejected_publications: row.get("rejected_publications"),
        })
    }

    pub async fn insert_candidates(
        &self,
        candidates: &[ContentCandidate],
    ) -> Result<i64, StoreError> {
        if candidates.is_empty() || candidates.len() > 100_000 {
            return Err(StoreError::Invalid("candidate count"));
        }
        let mut tx = self.pool.begin().await?;
        let mut revision = 0;
        for candidate in candidates {
            revision = sqlx::query_scalar("INSERT INTO content_candidates(wcid,class_name,weenie_type,payload) VALUES($1,$2,$3,$4) RETURNING revision")
                .bind(i64::from(candidate.wcid)).bind(&candidate.class_name).bind(candidate.weenie_type).bind(&candidate.bytes)
                .fetch_one(&mut *tx).await?;
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(revision)
    }
    /// Durable pending work, also used on startup/reconnect. Notifications are only wakeups.
    pub async fn pending_publications(
        &self,
        after: i64,
        limit: u32,
    ) -> Result<Vec<Publication>, StoreError> {
        if limit == 0 || limit > 1000 {
            return Err(StoreError::Invalid("poll limit"));
        }
        let revisions = sqlx::query("SELECT revision,payload_bytes FROM content_publications WHERE revision > $1 AND status='pending' ORDER BY revision LIMIT $2")
            .bind(after).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        let mut output = Vec::with_capacity(revisions.len());
        let mut total_bytes = 0_i64;
        for row in revisions {
            let revision: i64 = row.get("revision");
            let bytes: i64 = row.get("payload_bytes");
            if total_bytes + bytes > 256 * 1024 * 1024 {
                break;
            }
            total_bytes += bytes;
            let rows = sqlx::query("SELECT wcid,class_name,weenie_type,payload FROM content_candidates WHERE revision=$1 ORDER BY wcid")
                .bind(revision).fetch_all(&self.pool).await?;
            let candidates = rows
                .into_iter()
                .map(|row| ContentCandidate {
                    wcid: row.get::<i64, _>("wcid") as u32,
                    class_name: row.get("class_name"),
                    weenie_type: row.get("weenie_type"),
                    bytes: row.get("payload"),
                })
                .collect();
            output.push(Publication {
                revision,
                candidates,
            });
        }
        Ok(output)
    }
    /// Caller MUST decode and validate the complete candidate catalog before calling.
    pub async fn accept_validated(&self, revision: i64) -> Result<(), StoreError> {
        self.decide(revision, None).await
    }
    pub async fn reject(&self, revision: i64, reason: &str) -> Result<(), StoreError> {
        if reason.is_empty() || reason.len() > 4096 {
            return Err(StoreError::Invalid("rejection reason"));
        }
        self.decide(revision, Some(reason)).await
    }
    async fn decide(&self, revision: i64, rejection: Option<&str>) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        // One publication decider; this does not block journal allocation.
        sqlx::query("SELECT pg_advisory_xact_lock(42812001)")
            .execute(&mut *tx)
            .await?;
        let first: Option<i64> = sqlx::query_scalar("SELECT revision FROM content_publications WHERE status='pending' ORDER BY revision LIMIT 1")
            .fetch_optional(&mut *tx).await?;
        if first != Some(revision) {
            return Err(StoreError::PublicationOrder);
        }
        if rejection.is_none() {
            let mapped: bool = sqlx::query_scalar("SELECT active_manifest_hash IS NOT NULL FROM content_runtime_state WHERE singleton").fetch_one(&mut *tx).await?;
            if mapped {
                return Err(StoreError::Invalid("mapped content requires accept_mapped"));
            }
            sqlx::query("INSERT INTO content_heads(wcid,revision,class_name) SELECT wcid,revision,class_name FROM content_candidates WHERE revision=$1 ON CONFLICT(wcid) DO UPDATE SET revision=EXCLUDED.revision,class_name=EXCLUDED.class_name")
                .bind(revision).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE content_publications SET status=$2,rejection=$3 WHERE revision=$1")
            .bind(revision)
            .bind(if rejection.is_some() {
                "rejected"
            } else {
                "accepted"
            })
            .bind(rejection)
            .execute(&mut *tx)
            .await?;
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok(())
    }
    pub async fn active_content(&self) -> Result<Vec<ContentCandidate>, StoreError> {
        let rows = sqlx::query("SELECT c.wcid,c.class_name,c.weenie_type,c.payload FROM content_heads h JOIN content_candidates c USING(revision,wcid) ORDER BY c.wcid")
            .fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| ContentCandidate {
                wcid: row.get::<i64, _>("wcid") as u32,
                class_name: row.get("class_name"),
                weenie_type: row.get("weenie_type"),
                bytes: row.get("payload"),
            })
            .collect())
    }
    /// Consistent accepted generation and content, for restart/reconnect recovery.
    pub async fn active_catalog(&self) -> Result<(i64, Vec<ContentCandidate>), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision),0) FROM content_publications WHERE status='accepted'",
        )
        .fetch_one(&mut *tx)
        .await?;
        let rows = sqlx::query("SELECT c.wcid,c.class_name,c.weenie_type,c.payload FROM content_heads h JOIN content_candidates c USING(revision,wcid) ORDER BY c.wcid")
            .fetch_all(&mut *tx).await?;
        let candidates = rows
            .into_iter()
            .map(|row| ContentCandidate {
                wcid: row.get::<i64, _>("wcid") as u32,
                class_name: row.get("class_name"),
                weenie_type: row.get("weenie_type"),
                bytes: row.get("payload"),
            })
            .collect();
        tx.commit().await.map_err(crate::store::commit_error)?;
        Ok((revision, candidates))
    }
}
