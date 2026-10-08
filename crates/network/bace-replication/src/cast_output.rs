//! GDLE TryBeginCast/EndCast result codes carried by pinned ACE UseDone encoding.
//! Hardening-only failures use the source general-failure/busy response and never
//! imply that an effect or its durable resource write completed.
use crate::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError as E};
use bace_gameplay_api::{CastChange, CastOutcome, CastRejection, CharacterBinding};
pub fn cast_error_code(error: CastRejection) -> u32 {
    use CastRejection::*;
    match error {
        Busy | SchoolRecovery | StreakCooldown | PortalSpace | Capacity => 29,
        MissingActor => 55,
        UntrainedSchool => 1020,
        UnlearnedSpell => 1022,
        InvalidTarget => 1023,
        MissingComponents => 1024,
        InsufficientMana => 1025,
        Fizzled => 1026,
        Resisted => 0,
        WrongMode => 1034,
        OutOfRange => 1360,
        NotBound | OwnershipMismatch | StaleSequence | UnknownSpell | Obstructed | Dead
        | MissingAssets | InvalidState => 1033,
    }
}
impl EventSequencer {
    pub fn project_cast_outcome(
        &mut self,
        binding: CharacterBinding,
        outcome: &CastOutcome,
        limits: BatchLimits,
    ) -> Result<SessionBatch, E> {
        if self.binding != binding
            || outcome.context.actor != binding.actor
            || outcome.context.account != binding.account
            || outcome.context.session != binding.session
        {
            return Err(E::WrongBinding);
        }
        let code = match outcome.result {
            Ok(CastChange::Started { .. }) => {
                return Ok(SessionBatch {
                    binding,
                    messages: vec![],
                });
            }
            Ok(CastChange::Completed { .. } | CastChange::Cancelled { .. }) => 0,
            Err(error) => cast_error_code(error),
        };
        self.project_cast_done(binding, code, limits)
    }
    pub fn project_cast_done(
        &mut self,
        binding: CharacterBinding,
        code: u32,
        limits: BatchLimits,
    ) -> Result<SessionBatch, E> {
        if self.binding != binding {
            return Err(E::WrongBinding);
        }
        let mut messages = vec![];
        let mut total = 0;
        crate::session_output::push(
            &mut messages,
            &mut total,
            9,
            bace_wire::SimpleGameEvent::UseDone(code).encode(binding.actor.0, self.next),
            limits,
        )?;
        self.next = self.next.wrapping_add(1);
        Ok(SessionBatch { binding, messages })
    }
}
/// Current vital updates share the actor's existing private property counter.
pub fn project_magic_vital(
    binding: CharacterBinding,
    vital: u32,
    current: u32,
    sequences: &mut crate::Sequences,
    limits: BatchLimits,
) -> Result<SessionBatch, E> {
    if !matches!(vital, 2 | 4 | 6) {
        return Err(E::InvalidProjection);
    }
    let sequence = (sequences.current(crate::SequenceKind::Vital, vital) as u8).wrapping_add(1);
    let mut messages = vec![];
    let mut total = 0;
    crate::session_output::push(
        &mut messages,
        &mut total,
        9,
        bace_wire::CurrentVitalUpdate {
            sequence,
            vital,
            current,
        }
        .encode(),
        limits,
    )?;
    sequences
        .advance_batch([(crate::SequenceKind::Vital, vital)])
        .map_err(|_| E::Limit)?;
    Ok(SessionBatch { binding, messages })
}
