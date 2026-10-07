//! Database-to-catalog publication. Invoke on an asynchronous worker, never the world thread.
use bace_content::{Catalog, CatalogSnapshot, ContentError, ContentLimits, WeenieTemplate};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::ContentCandidate;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Content(#[from] ContentError),
    #[error("content decoding failed: {0}")]
    Decode(#[from] bace_content_tools::ToolError),
    #[error("binary identity disagrees with database scalar columns for WCID {0}")]
    Identity(u32),
    #[error("invalid database content generation")]
    Revision,
    #[error("catalog worker failed: {0}")]
    Worker(#[from] tokio::task::JoinError),
    #[error("tick-boundary catalog receiver is closed")]
    ReceiverClosed,
}
#[derive(Debug, PartialEq, Eq)]
pub enum PublicationResult {
    Idle,
    Accepted { revision: i64 },
    Rejected { revision: i64, reason: String },
}

/// Startup/recovery loads one consistent persisted last-known-good generation.
/// Invalid accepted data fails readiness; startup never falls back to empty content.
pub async fn load_catalog(store: &PgStore) -> Result<Catalog, PublicationError> {
    let (revision, candidates) = store.active_catalog().await?;
    tokio::task::spawn_blocking(move || {
        let mut catalog = Catalog::default();
        if revision == 0 && candidates.is_empty() {
            return Ok(catalog);
        }
        let revision = u64::try_from(revision).map_err(|_| PublicationError::Revision)?;
        let prepared = catalog.prepare(
            revision,
            decode_candidates(candidates)?,
            ContentLimits::default(),
        )?;
        catalog.publish(prepared)?;
        Ok(catalog)
    })
    .await?
}

/// Process the oldest pending transaction. A rejected batch does not strand later work.
/// The bounded channel is consumed by the world owner only at a tick boundary.
/// On uncertain commit error the worker MUST reload_catalog before resuming publication.
pub async fn publish_pending_once(
    store: &PgStore,
    catalog: &mut Catalog,
    delivery: &mpsc::Sender<Arc<CatalogSnapshot>>,
) -> Result<PublicationResult, PublicationError> {
    let Some(pending) = store.pending_publications(0, 1).await?.into_iter().next() else {
        return Ok(PublicationResult::Idle);
    };
    let revision = pending.revision;
    let base = catalog.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        let revision = u64::try_from(revision).map_err(|_| PublicationError::Revision)?;
        Ok::<_, PublicationError>(base.prepare(
            revision,
            decode_candidates(pending.candidates)?,
            ContentLimits::default(),
        )?)
    })
    .await?;
    let prepared = match prepared {
        Ok(value) => value,
        Err(error) => {
            // Bound persisted diagnostics by UTF-8 characters (at most 4096 bytes).
            let reason: String = error.to_string().chars().take(1024).collect();
            store.reject(revision, &reason).await?;
            return Ok(PublicationResult::Rejected { revision, reason });
        }
    };
    // Reserve delivery space before durability changes. Backpressure never stalls the world.
    let permit = delivery
        .reserve()
        .await
        .map_err(|_| PublicationError::ReceiverClosed)?;
    store.accept_validated(revision).await?;
    let snapshot = catalog.publish(prepared)?;
    permit.send(snapshot);
    Ok(PublicationResult::Accepted { revision })
}

fn decode_candidates(
    candidates: Vec<ContentCandidate>,
) -> Result<Vec<WeenieTemplate>, PublicationError> {
    candidates
        .into_iter()
        .map(|candidate| {
            let template = bace_content_tools::decode(&candidate.bytes)?;
            if template.weenie_id != candidate.wcid
                || template.class_name != candidate.class_name
                || i32::try_from(template.weenie_type).ok() != Some(candidate.weenie_type)
            {
                return Err(PublicationError::Identity(candidate.wcid));
            }
            Ok(template)
        })
        .collect()
}
