//! Owner-confirmed jump stamina and source tiredness error use the same canonical
//! session/property counters as all other private player output.
use crate::{
    BatchLimits, EventSequencer, SequenceKind, Sequences, SessionBatch, SessionProjectionError as E,
};
use bace_gameplay_api::{
    CharacterBinding,
    locomotion::{LocomotionOutcome, LocomotionRejection},
};
impl EventSequencer {
    pub fn project_locomotion_outcome(
        &mut self,
        binding: CharacterBinding,
        outcome: &LocomotionOutcome,
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
        let mut messages = Vec::new();
        let mut bytes = 0;
        let mut vital = false;
        let mut event = false;
        match &outcome.result {
            Ok(accepted) => {
                if let Some((_, current)) = accepted.stamina {
                    crate::session_output::push(
                        &mut messages,
                        &mut bytes,
                        9,
                        bace_wire::CurrentVitalUpdate {
                            sequence: (properties.current(SequenceKind::Vital, 4) as u8)
                                .wrapping_add(1),
                            vital: 4,
                            current,
                        }
                        .encode(),
                        limits,
                    )?;
                    vital = true;
                }
            }
            Err(LocomotionRejection::TooTired) => {
                // Source jump stamina failure, documented in the character oracle.
                crate::session_output::push(
                    &mut messages,
                    &mut bytes,
                    9,
                    bace_wire::SimpleGameEvent::WeenieError(0x3e)
                        .encode(binding.actor.0, self.next),
                    limits,
                )?;
                event = true;
            }
            Err(_) => {} // Accepted-world correction is owned by visibility.
        }
        if vital {
            properties
                .advance_batch([(SequenceKind::Vital, 4)])
                .map_err(|_| E::Limit)?;
        }
        if event {
            self.next = self.next.wrapping_add(1);
        }
        Ok(SessionBatch { binding, messages })
    }
}
