//! Source combat result projection through the session's canonical counters.
use crate::{
    BatchLimits, EventSequencer, SequenceKind, Sequences, SessionBatch, SessionProjectionError as E,
};
use bace_gameplay_api::{CharacterBinding, CombatChange, CombatOutcome, CombatRejection};
/// GDLE GameEnums.h WErrorType. Authority-only rejections use BAD_PARAM;
/// a rejection never claims an attack or ammunition transaction completed.
pub fn combat_error_code(error: CombatRejection) -> u32 {
    use CombatRejection::*;
    match error {
        Busy | Capacity => 29,
        MissingActor => 55,
        Dead => 58,
        OutOfRange => 61,
        Obstructed => 57,
        NotBound | OwnershipMismatch | StaleSequence | InvalidRequest | UnsupportedMode
        | MissingCombatProfile => 2,
    }
}
impl EventSequencer {
    pub fn project_combat_outcome(
        &mut self,
        binding: CharacterBinding,
        outcome: &CombatOutcome,
        properties: &mut Sequences,
        limits: BatchLimits,
    ) -> Result<SessionBatch, E> {
        if self.binding != binding
            || outcome.context.actor != binding.actor
            || outcome.context.account != binding.account
            || outcome.context.session != binding.session
        {
            return Err(E::WrongBinding);
        }
        match outcome.result {
            Err(error) => self.project_combat(
                binding,
                &[bace_wire::CombatEvent::AttackDone(combat_error_code(error))],
                limits,
            ),
            Ok(CombatChange::Mode(mode)) => {
                let mut messages = vec![];
                let mut bytes = 0;
                crate::session_output::push(
                    &mut messages,
                    &mut bytes,
                    9,
                    bace_wire::PropertyUpdate {
                        sequence: (properties.current(SequenceKind::PropertyInt, 40) as u8)
                            .wrapping_add(1),
                        object_id: None,
                        property: 40,
                        value: bace_wire::PropertyValue::Int(mode as i32),
                    }
                    .encode()?,
                    limits,
                )?;
                properties
                    .advance(SequenceKind::PropertyInt, 40)
                    .map_err(|_| E::Limit)?;
                Ok(SessionBatch { binding, messages })
            }
            // Accepted attack/cancel notifications come from the ordered attack
            // lifecycle. Emitting another AttackDone here would duplicate it.
            Ok(CombatChange::AttackStarted { .. } | CombatChange::Cancelled) => Ok(SessionBatch {
                binding,
                messages: vec![],
            }),
            Ok(CombatChange::Health { .. }) => Err(E::InvalidProjection),
        }
    }
}
