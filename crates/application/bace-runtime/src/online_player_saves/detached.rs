//! Final immutable checkpoint after atomic simulation ownership transfer.
use super::*;
impl OnlinePlayerSaveService {
    pub fn owns_capture(&self, correlation: u64) -> bool {
        self.capture
            .is_some_and(|(token, _, _)| token == correlation)
    }
    pub fn stage_detached_snapshot(
        &mut self,
        snapshot: std::sync::Arc<bace_simulation::PlayerReadSnapshot>,
        now: Duration,
        unix_millis: u64,
    ) -> Result<(), String> {
        let actor = snapshot.binding().actor.0;
        if !self.critical_ready(&[actor])? || self.capture.is_some() {
            return Err("routine work precedes detached checkpoint".into());
        }
        let p = self
            .players
            .get(&actor)
            .ok_or("detached save actor missing")?;
        if p.binding != snapshot.binding() || snapshot.operation().is_some() {
            return Err("detached snapshot binding mismatch".into());
        }
        let token = self
            .next
            .checked_add(1)
            .ok_or("detached capture token exhausted")?;
        self.capture = Some((token, actor, now));
        self.next = token;
        let p = self.players.get_mut(&actor).expect("validated player");
        p.requested_notice = p.notice;
        self.accept_capture(
            PlayerSnapshotOutcome {
                correlation: token,
                result: Ok(snapshot),
            },
            unix_millis,
        )
        .map_err(|(error, _)| error)?;
        let p = self.players.get_mut(&actor).expect("retained player");
        p.detached = true;
        p.drain_at = Some(now);
        Ok(())
    }
}
