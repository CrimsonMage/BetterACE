use std::collections::BTreeMap;

use bace_character::CharacterProgression;
use bace_gameplay_api::{
    ActionContext, ActionResult, CharacterBinding, ProgressionActionRejection, ProgressionOutcome,
    RaiseProgression,
};
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterRegistrationError {
    MissingActor,
    Capacity,
    ActorAlreadyBound,
    AccountAlreadyBound,
    SessionAlreadyBound,
    OwnershipMismatch,
}

struct OwnedCharacter {
    binding: CharacterBinding,
    progression: CharacterProgression,
    last_sequence: Option<u32>,
}

/// Character state lives on the same Kernel owner as world/physics; no Body or
/// accepted pose is copied here. Registration work is bounded by capacity.
pub(crate) struct Characters {
    entries: BTreeMap<EntityId, OwnedCharacter>,
    capacity: usize,
}

impl Characters {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity,
        }
    }

    pub(crate) fn register(
        &mut self,
        binding: CharacterBinding,
        progression: CharacterProgression,
    ) -> Result<(), (CharacterRegistrationError, CharacterProgression)> {
        let error = if self.entries.contains_key(&binding.actor) {
            Some(CharacterRegistrationError::ActorAlreadyBound)
        } else if self
            .entries
            .values()
            .any(|entry| entry.binding.account == binding.account)
        {
            Some(CharacterRegistrationError::AccountAlreadyBound)
        } else if self
            .entries
            .values()
            .any(|entry| entry.binding.session == binding.session)
        {
            Some(CharacterRegistrationError::SessionAlreadyBound)
        } else if self.entries.len() >= self.capacity {
            Some(CharacterRegistrationError::Capacity)
        } else {
            None
        };
        if let Some(error) = error {
            return Err((error, progression));
        }
        self.entries.insert(
            binding.actor,
            OwnedCharacter {
                binding,
                progression,
                last_sequence: None,
            },
        );
        Ok(())
    }

    pub(crate) fn take(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<CharacterProgression, CharacterRegistrationError> {
        if self
            .entries
            .get(&binding.actor)
            .is_none_or(|entry| entry.binding != binding)
        {
            return Err(CharacterRegistrationError::OwnershipMismatch);
        }
        Ok(self
            .entries
            .remove(&binding.actor)
            .expect("checked entry")
            .progression)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn get(&self, actor: EntityId) -> Option<&CharacterProgression> {
        self.entries.get(&actor).map(|entry| &entry.progression)
    }

    pub(crate) fn apply(
        &mut self,
        context: ActionContext,
        request: RaiseProgression,
        actor_exists: bool,
    ) -> ProgressionOutcome {
        let result = self.apply_inner(context, request, actor_exists);
        ActionResult { context, result }
    }

    fn apply_inner(
        &mut self,
        context: ActionContext,
        request: RaiseProgression,
        actor_exists: bool,
    ) -> Result<bace_gameplay_api::ProgressionChange, ProgressionActionRejection> {
        let entry = self
            .entries
            .get_mut(&context.actor)
            .ok_or(ProgressionActionRejection::NotBound)?;
        if entry.binding.account != context.account || entry.binding.session != context.session {
            return Err(ProgressionActionRejection::OwnershipMismatch);
        }
        if !actor_exists {
            return Err(ProgressionActionRejection::MissingActor);
        }
        if let Some(last) = entry.last_sequence {
            let distance = context.sequence.wrapping_sub(last);
            if distance == 0 || distance >= 0x8000_0000 {
                return Err(ProgressionActionRejection::StaleSequence);
            }
        }
        // Consume authenticated attempts, even rejected expenditures, so the
        // same rejected request cannot become effective after state changes.
        entry.last_sequence = Some(context.sequence);
        entry
            .progression
            .raise(request)
            .map_err(ProgressionActionRejection::Domain)
    }
}
