//! Shared event-counter projection, including observed retail salvage cardinality.
use crate::session_output::{
    BatchLimits, EventSequencer, SessionBatch, SessionProjectionError, push,
};
use bace_gameplay_api::CharacterBinding;
use bace_wire::{CraftingEvent, SalvageWireResult};

impl EventSequencer {
    /// Encode and validate the entire batch before advancing the shared counter.
    pub fn project_crafting(
        &mut self,
        binding: CharacterBinding,
        events: &[CraftingEvent<'_>],
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if events.len() > limits.max_messages || events.len() > 300 {
            return Err(SessionProjectionError::Limit);
        }
        let mut messages = Vec::with_capacity(events.len());
        let mut total = 0;
        for (index, event) in events.iter().enumerate() {
            if let CraftingEvent::ConfirmationRequest { text, .. } = event
                && text.len() > limits.max_string_bytes
            {
                return Err(SessionProjectionError::Limit);
            }
            let bytes = event.encode(
                binding.actor.0,
                self.next.wrapping_add(index as u32),
                limits.max_message_bytes,
            )?;
            push(&mut messages, &mut total, 9, bytes, limits)?;
        }
        self.next = self.next.wrapping_add(events.len() as u32);
        Ok(SessionBatch { binding, messages })
    }

    /// One result per bag and one empty acknowledgment when no bags were made.
    /// Unsuitable identities appear exactly once, on the first acknowledgment.
    /// Caller must supply only durably committed bags or a no-mutation result.
    pub fn project_salvage_results(
        &mut self,
        binding: CharacterBinding,
        skill: u32,
        unsuitable: &[u32],
        bags: &[SalvageWireResult],
        augmentation_bonus: u32,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if bags.len() > 300 || unsuitable.len() > 300 || bags.len().max(1) > limits.max_messages {
            return Err(SessionProjectionError::Limit);
        }
        if bags.iter().any(|bag| bag.units == 0 || bag.units > 100) {
            return Err(SessionProjectionError::InvalidProjection);
        }
        let events: Vec<_> = if bags.is_empty() {
            vec![CraftingEvent::SalvageResult {
                skill,
                unsuitable,
                result: None,
                augmentation_bonus,
            }]
        } else {
            bags.iter()
                .enumerate()
                .map(|(index, bag)| CraftingEvent::SalvageResult {
                    skill,
                    unsuitable: if index == 0 { unsuitable } else { &[] },
                    result: Some(*bag),
                    augmentation_bonus,
                })
                .collect()
        };
        self.project_crafting(binding, &events, limits)
    }
}
