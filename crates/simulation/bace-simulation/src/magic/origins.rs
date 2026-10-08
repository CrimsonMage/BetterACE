use super::*;
pub(super) fn origin_identity(origin: CastOrigin) -> [u8; 16] {
    let mut identity = [0; 16];
    match origin {
        CastOrigin::Player(context) => {
            identity[..8].copy_from_slice(&context.session.0.to_le_bytes());
            identity[8..12].copy_from_slice(&context.actor.0.to_le_bytes());
            identity[12..].copy_from_slice(&context.sequence.to_le_bytes());
        }
        _ => {
            let (kind, event) = origin.server_event().expect("server variant");
            identity[..8].copy_from_slice(&event.to_le_bytes());
            identity[8..12].copy_from_slice(&origin.actor().0.to_le_bytes());
            identity[12] = kind;
            identity[15] = 0x80;
        }
    }
    identity
}
impl Magic {
    pub(super) fn admit_server_origin(&mut self, origin: CastOrigin) {
        if let Some((kind, event)) = origin.server_event() {
            let last = self
                .server_sequences
                .entry((origin.actor(), kind))
                .or_default();
            *last = (*last).max(event);
            if let Some(request) = self.item_procs.iter_mut().find(|p| p.origin == origin) {
                request.admitted = true;
            }
        }
    }
    pub(super) fn publish_outcome(
        &mut self,
        origin: CastOrigin,
        result: Result<CastChange, CastRejection>,
    ) {
        match origin {
            CastOrigin::Player(context) => {
                self.outcomes.push_back(ActionResult { context, result })
            }
            _ => self
                .server_outcomes
                .push_back(ServerCastOutcome { origin, result }),
        }
    }
    pub(crate) fn claim_emote_outcome(
        &mut self,
        actor: EntityId,
        event: u64,
    ) -> Result<(), CastRejection> {
        if self.claimed_emote_outcomes.contains(&(actor, event)) {
            return Err(CastRejection::Busy);
        }
        if self.claimed_emote_outcomes.len() >= 4096 {
            return Err(CastRejection::Capacity);
        }
        self.claimed_emote_outcomes.insert((actor, event));
        Ok(())
    }
    pub(crate) fn release_emote_outcome(&mut self, actor: EntityId, event: u64) {
        self.claimed_emote_outcomes.remove(&(actor, event));
    }
    pub(crate) fn take_item_proc_outcome(
        &mut self,
        origin: CastOrigin,
    ) -> Option<ServerCastOutcome> {
        let index = self
            .server_outcomes
            .iter()
            .position(|o| o.origin == origin)?;
        self.server_outcomes.remove(index)
    }
    pub(crate) fn take_server_outcome(&mut self) -> Option<ServerCastOutcome> {
        let index=self.server_outcomes.iter().position(|outcome| !self.item_procs.iter().any(|p|p.origin==outcome.origin) && !matches!(outcome.origin,CastOrigin::Emote{actor,event,..}if self.claimed_emote_outcomes.contains(&(actor,event))))?;
        self.server_outcomes.remove(index)
    }
    /// Consume only this delegated action's receipt; other server owners retain
    /// their outcomes and original queue order.
    pub(crate) fn take_emote_outcome(
        &mut self,
        actor: EntityId,
        event: u64,
    ) -> Option<ServerCastOutcome> {
        let index = self.server_outcomes.iter().position(|outcome| {
            matches!(outcome.origin, CastOrigin::Emote { actor: a, event: e, .. } if a == actor && e == event)
        })?;
        self.server_outcomes.remove(index)
    }
}
