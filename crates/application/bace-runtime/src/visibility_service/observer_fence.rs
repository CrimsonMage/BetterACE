//! A gameplay publication fence cancels cold reads, never accepted wire packets.
use super::*;
impl VisibilityService {
    pub fn poll_pending(
        &mut self,
        worker: &SimulationWorker,
        players: &mut PlayerService,
        network: &NetworkThread,
    ) -> Result<(), VisibilityServiceError> {
        let bindings: Vec<_> = players.entered_bindings().collect();
        let gone: Vec<_> = self
            .observers
            .keys()
            .filter(|key| !bindings.iter().any(|(k, _)| k == *key))
            .copied()
            .collect();
        for key in gone {
            self.unbind(key);
        }
        // No new observers/queries or projections are started under the fence.
        for observer in self.observers.values_mut() {
            if observer.view_request.take().is_some() {
                discard_read(observer)?;
            }
        }
        for _ in 0..256 {
            let Ok(outcome) = worker.visibility_outcomes().try_recv() else {
                break;
            };
            let outcome =
                Arc::try_unwrap(outcome).map_err(|_| VisibilityServiceError::Projection)?;
            self.cancel_visibility_read(outcome)?;
        }
        for _ in 0..256 {
            let Ok(outcome) = worker.object_view_outcomes().try_recv() else {
                break;
            };
            self.cancel_view_read(outcome.correlation)?;
        }
        self.drive_pending(&worker.input(), network)
    }
    fn cancel_visibility_read(
        &mut self,
        outcome: VisibilityOutcome,
    ) -> Result<(), VisibilityServiceError> {
        let Some(observer) = self
            .observers
            .values_mut()
            .find(|o| o.query == Some(outcome.correlation))
        else {
            // The generation was unbound; no knowledge or packets survived it.
            return Ok(());
        };
        if observer.publication.is_some() || observer.knowledge.pending().is_some() {
            return Err(VisibilityServiceError::Projection);
        }
        observer.query = None;
        observer.buffer = match outcome.result {
            Ok(snapshot) => snapshot.candidates,
            Err((_, buffer)) => buffer,
        };
        Ok(())
    }
    fn cancel_view_read(&mut self, correlation: u64) -> Result<(), VisibilityServiceError> {
        let Some(observer) = self
            .observers
            .values_mut()
            .find(|o| o.view_query == Some(correlation))
        else {
            return Ok(());
        };
        discard_read(observer)?;
        observer.view_query = None;
        Ok(())
    }
}
fn discard_read(observer: &mut Observer) -> Result<(), VisibilityServiceError> {
    if observer.publication.is_some() {
        return Err(VisibilityServiceError::Projection);
    }
    let ticket = observer
        .knowledge
        .pending()
        .ok_or(VisibilityServiceError::Projection)?
        .ticket;
    if let Some(snapshot) = observer
        .knowledge
        .discard_unpublished(ticket)
        .map_err(|_| VisibilityServiceError::Projection)?
    {
        observer.buffer = snapshot.candidates;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
