//! Query outputs share the one authenticated event counter.
use crate::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError};
use bace_gameplay_api::{
    CharacterBinding,
    selection::{TargetQueryEvent, TargetQueryResponse},
};
impl EventSequencer {
    pub fn project_target_query(
        &mut self,
        binding: CharacterBinding,
        event: TargetQueryEvent,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if binding.actor != event.context.actor
            || binding.account != event.context.account
            || binding.session != event.context.session
        {
            return Err(SessionProjectionError::WrongBinding);
        }
        let mut messages = vec![];
        let mut total = 0;
        if let Some(response) = event.response {
            let bytes = match response {
                TargetQueryResponse::Health { target, fraction } => {
                    bace_wire::CombatEvent::UpdateHealth {
                        target_id: target.0,
                        fraction,
                    }
                    .encode(
                        binding.actor.0,
                        self.next,
                        limits.max_string_bytes,
                        limits.max_message_bytes,
                    )?
                }
                TargetQueryResponse::ItemMana {
                    target,
                    fraction,
                    success,
                } => bace_wire::encode_item_mana_query(
                    binding.actor.0,
                    self.next,
                    target.0,
                    fraction,
                    success,
                    limits.max_message_bytes,
                )?,
            };
            crate::session_output::push(&mut messages, &mut total, 9, bytes, limits)?;
            self.next = self.next.wrapping_add(1);
        }
        Ok(SessionBatch { binding, messages })
    }
}
