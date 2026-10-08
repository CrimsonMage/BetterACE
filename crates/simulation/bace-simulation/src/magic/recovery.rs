//! Actor-owned cast recovery survives attempt completion and reconnect transfer.
use super::*;
impl Magic {
    /// Read the committed recovery clock even while a later cast is in flight.
    /// A cast only records recovery at its normal authoritative transition.
    pub(crate) fn read_recovery_snapshot(
        &self,
        actor: EntityId,
        now: f64,
    ) -> Result<bace_magic::CastRecovery, CastRejection> {
        self.recovery
            .get(&actor)
            .copied()
            .ok_or(CastRejection::MissingActor)?
            .snapshot(now)
            .map_err(rejection)
    }
    pub(crate) fn recovery_snapshot(
        &self,
        actor: EntityId,
        now: f64,
    ) -> Result<bace_magic::CastRecovery, CastRejection> {
        if self.busy(actor) {
            return Err(CastRejection::Busy);
        }
        self.recovery
            .get(&actor)
            .copied()
            .ok_or(CastRejection::MissingActor)?
            .snapshot(now)
            .map_err(rejection)
    }
    pub(crate) fn restore_recovery(
        &mut self,
        actor: EntityId,
        snapshot: bace_magic::CastRecovery,
        now: f64,
        elapsed: f64,
    ) -> Result<(), CastRejection> {
        if self.busy(actor) {
            return Err(CastRejection::Busy);
        }
        let previous = self
            .recovery
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if previous.snapshot(now).map_err(rejection)?.revision != 0 {
            return Err(CastRejection::InvalidState);
        }
        let restored =
            bace_magic::CastRecoveryClock::restore(snapshot, now, elapsed).map_err(rejection)?;
        self.recovery.insert(actor, restored);
        Ok(())
    }
    pub(super) fn retain_recovery(
        &mut self,
        attempt: &Attempt,
        success: Option<bace_magic::MagicSchool>,
        now: f64,
    ) -> Result<(), CastRejection> {
        let (minimum, streak) = attempt.driver.recovery_deadlines();
        self.recovery
            .get_mut(&attempt.origin.actor())
            .ok_or(CastRejection::MissingActor)?
            .record(minimum, streak, success, now)
            .map_err(rejection)
    }
}
impl Magic {
    pub(crate) fn recovery_revision(&self, actor: EntityId) -> Option<u64> {
        self.recovery
            .get(&actor)?
            .snapshot(self.current_time)
            .ok()
            .map(|s| s.revision)
    }
    pub(crate) fn can_take_actor_recovery(
        &self,
        actor: EntityId,
        now: f64,
    ) -> Result<bace_magic::CastRecovery, CastRejection> {
        if self.busy(actor)
            || self
                .instant_continuations
                .values()
                .any(|a| a.origin.actor() == actor || a.target == Some(actor))
            || self.outcomes.iter().any(|o| o.context.actor == actor)
            || self
                .server_outcomes
                .iter()
                .any(|o| o.origin.actor() == actor)
            || self
                .component_operations
                .values()
                .any(|binding| binding.actor == actor)
            || self
                .events
                .iter()
                .any(|e| registry::event_actor(e) == actor)
        {
            return Err(CastRejection::Busy);
        }
        self.recovery_snapshot(actor, now)
    }
    pub(crate) fn take_actor_recovery(
        &mut self,
        actor: EntityId,
        now: f64,
    ) -> Result<bace_magic::CastRecovery, CastRejection> {
        let snapshot = self.can_take_actor_recovery(actor, now)?;
        self.recovery.remove(&actor);
        self.casters.remove(&actor);
        self.monster_ai.remove(&actor);
        self.actor_components.retain(|(id, _), _| *id != actor);
        self.validated_actor_programs.retain(|(id, _)| *id != actor);
        self.server_sequences.retain(|(id, _), _| *id != actor);
        Ok(snapshot)
    }
}

impl Magic {
    pub(crate) fn configure_execution_epoch(&mut self, epoch: u64) -> Result<(), CastRejection> {
        if epoch == 0
            || !self.casters.is_empty()
            || !self.attempts.is_empty()
            || !self.flying.is_empty()
        {
            return Err(CastRejection::InvalidState);
        }
        self.execution_epoch = epoch;
        Ok(())
    }
}
