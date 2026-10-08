use super::*;
impl Characters {
    pub(crate) fn validate_player_admission(
        &self,
        binding: CharacterBinding,
        state: &OwnedCharacterState,
    ) -> Result<(), CharacterRegistrationError> {
        if binding.actor.0 == 0 || binding.account.0 == 0 || binding.session.0 == 0 {
            return Err(CharacterRegistrationError::OwnershipMismatch);
        }
        if self.entries.contains_key(&binding.actor) {
            return Err(CharacterRegistrationError::ActorAlreadyBound);
        }
        if self
            .entries
            .values()
            .any(|e| e.binding.account == binding.account)
        {
            return Err(CharacterRegistrationError::AccountAlreadyBound);
        }
        if self
            .entries
            .values()
            .any(|e| e.binding.session == binding.session)
        {
            return Err(CharacterRegistrationError::SessionAlreadyBound);
        }
        if self.entries.len() >= self.capacity {
            return Err(CharacterRegistrationError::Capacity);
        }
        let ui = state
            .ui
            .as_ref()
            .ok_or(CharacterRegistrationError::OwnershipMismatch)?;
        if ui.known_spells.len() > 4096
            || ui.component_templates.len() > 4096
            || bace_character::ui::validate(&ui.state).is_err()
            || state
                .native_services
                .as_ref()
                .is_none_or(|s| s.validate().is_err())
            || state.contracts.is_none()
            || state.rares.is_some_and(|r| {
                r.character != binding.actor.0 || r.random_identity == [0; 16] || r.key_version == 0
            })
        {
            return Err(CharacterRegistrationError::OwnershipMismatch);
        }
        Ok(())
    }
}
