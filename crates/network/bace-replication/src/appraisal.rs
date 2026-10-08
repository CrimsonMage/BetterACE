//! One canonical private IdentifyObjectResponse event after accepted gameplay.
use crate::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError};
use bace_gameplay_api::CharacterBinding;
use bace_wire::{AppraisalLimits, AppraisalProfile};

impl EventSequencer {
    pub fn project_appraisal(
        &mut self,
        binding: CharacterBinding,
        target: u32,
        profile: &AppraisalProfile,
        codec: AppraisalLimits,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if target == 0 {
            return Err(SessionProjectionError::InvalidProjection);
        }
        let bytes = profile.encode(binding.actor.0, self.next, target, codec)?;
        let mut messages = Vec::with_capacity(1);
        let mut total = 0;
        crate::session_output::push(&mut messages, &mut total, 9, bytes, limits)?;
        self.next = self.next.wrapping_add(1);
        Ok(SessionBatch { binding, messages })
    }
}
