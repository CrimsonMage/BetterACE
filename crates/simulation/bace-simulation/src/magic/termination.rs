//! Terminal outcomes wait for owner-admitted Ready transitions; effects never replay.
use super::*;
pub(super) struct TerminalCast {
    result: Result<CastChange, CastRejection>,
}
impl Magic {
    pub(super) fn finish_attempt(
        &mut self,
        attempt: &mut Attempt,
        result: Result<CastChange, CastRejection>,
        world: &mut World,
        now: f64,
    ) -> Result<(), CastRejection> {
        if attempt.terminal.is_none() {
            self.retain_recovery(
                attempt,
                (result.is_ok() && matches!(attempt.origin, CastOrigin::Player(_)))
                    .then_some(attempt.prepared.spell.school),
                now,
            )?;
            attempt.terminal = Some(TerminalCast { result });
        }
        self.flush_terminal(attempt, world);
        Ok(())
    }
    pub(super) fn flush_terminal(&mut self, attempt: &mut Attempt, world: &mut World) {
        if !self.can_accept() || attempt.terminal.is_none() {
            return;
        }
        // The retained World action and Ready suffix remain the same owner.
        // An admission failure retains the result, not a live effect to rerun.
        if !attempt.origin.instant()
            && world
                .combatant(attempt.origin.actor())
                .is_none_or(|c| c.health() != 0)
            && self
                .admit_cast_stop(attempt.origin.actor(), attempt.cast, world)
                .is_err()
        {
            return;
        }
        restore_monster_mode(attempt, world);
        if !attempt.origin.instant()
            && let Ok(body) = world.body_mut(attempt.origin.actor())
        {
            body.stop_motion();
        }
        let terminal = attempt.terminal.take().expect("checked terminal cast");
        self.publish_outcome(attempt.origin, terminal.result);
    }
}

impl Magic {
    pub(super) fn finish_fizzled_attempt(
        &mut self,
        attempt: &mut Attempt,
        world: &mut World,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.retain_recovery(attempt, None, now)?;
        attempt.terminal = Some(TerminalCast {
            result: Ok(CastChange::Completed { cast: attempt.cast }),
        });
        self.flush_terminal(attempt, world);
        Ok(())
    }
}

impl Magic {
    pub(super) fn admit_cast_stop(
        &mut self,
        actor: EntityId,
        cast: u64,
        world: &mut World,
    ) -> Result<(), CastRejection> {
        if self.events.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        world
            .end_cast_motion(actor, cast)
            .map_err(|_| CastRejection::InvalidState)?;
        if let (Some(token), Some(state)) = (
            world.source_motion_token(actor),
            world.source_motion_state(actor),
        ) {
            self.events.push_back(MagicEvent::MotionStopped {
                actor,
                cast,
                sequence: token.sequence,
                style: state.style,
                substate: state.substate,
                speed: state.speed,
            });
        }
        Ok(())
    }
}
