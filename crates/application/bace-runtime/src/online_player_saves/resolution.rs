//! Bounded I/O handoff for uncertain routine outcomes; the controller keeps its
//! original payload while unrelated routine actors continue to receive service.
use super::*;
#[derive(Clone, Debug)]
pub struct RoutineResolutionRequest {
    actor: u32,
    batch: std::sync::Arc<OwnedSaveBatch>,
}
pub struct RoutineResolutionOutcome {
    request: RoutineResolutionRequest,
    result: Result<Option<Vec<SaveAck>>, String>,
}
impl RoutineResolutionRequest {
    /// Execute on a free bounded I/O slot. Cancellation cannot erase the original
    /// uncertain request retained by OnlinePlayerSaveService.
    pub async fn resolve(self, store: &bace_db_postgres::PgStore) -> RoutineResolutionOutcome {
        let result = store
            .resolve_owned_batch(&self.batch)
            .await
            .map_err(|e| e.to_string());
        RoutineResolutionOutcome {
            request: self,
            result,
        }
    }
}
impl OnlinePlayerSaveService {
    /// Call only when the host's bounded resolution lane can retain the job.
    /// This clones at most one already byte-bounded batch per uncertain retry,
    /// never one payload per actor per tick. Requests may safely be queried again.
    pub fn resolution_request(&mut self) -> Option<RoutineResolutionRequest> {
        let selected = self
            .writes
            .range((
                std::ops::Bound::Excluded(self.poll_cursor),
                std::ops::Bound::Unbounded,
            ))
            .chain(self.writes.range(..=self.poll_cursor))
            .find(|(_, p)| p.uncertain)
            .map(|(&id, p)| (id, p.batch.clone()))?;
        self.poll_cursor = selected.0;
        Some(RoutineResolutionRequest {
            actor: selected.0,
            batch: std::sync::Arc::new(selected.1),
        })
    }
    pub fn accept_resolution(&mut self, outcome: RoutineResolutionOutcome) -> Result<(), String> {
        let actor = outcome.request.actor;
        let pending = self.writes.get(&actor).ok_or("stale routine resolution")?;
        let expected = &pending.batch;
        let request = &outcome.request.batch;
        if !pending.uncertain
            || expected.participants != request.participants
            || expected.leases != request.leases
            || expected.snapshots != request.snapshots
        {
            return Err("routine resolution request mismatch".into());
        }
        match outcome.result {
            Ok(Some(acks)) => self.accept_acks(actor, &acks),
            Ok(None) => {
                let pending = self
                    .writes
                    .remove(&actor)
                    .expect("validated unresolved batch");
                for s in &pending.batch.snapshots {
                    self.dirty.failed(s).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            Err(error) => {
                self.degraded = Some(error.clone());
                Err(error)
            }
        }
    }
}
