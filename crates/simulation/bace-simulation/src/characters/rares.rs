use super::*;
use bace_gameplay_api::{CharacterRareState, RareDecision};
/// Lifecycle transfer includes every RNG cursor; the legacy progression-only
/// transfer refuses characters carrying this additional authoritative state.
#[derive(Debug)]
pub struct OwnedCharacterState {
    pub progression: CharacterProgression,
    pub rares: Option<CharacterRareState>,
    pub ui: Option<OwnedUiState>,
    pub native_services: Option<bace_character::CharacterServiceState>,
    pub contracts: Option<bace_quests::ContractRegistry>,
}
impl Characters {
    pub(crate) fn rare(&self, actor: EntityId) -> Option<CharacterRareState> {
        self.entries.get(&actor).and_then(|e| e.rare)
    }
    pub(crate) fn register_complete(
        &mut self,
        binding: CharacterBinding,
        state: OwnedCharacterState,
    ) -> Result<(), (CharacterRegistrationError, Box<OwnedCharacterState>)> {
        if state.rares.is_some_and(|r| {
            r.character != binding.actor.0 || r.random_identity == [0; 16] || r.key_version == 0
        }) {
            return Err((
                CharacterRegistrationError::OwnershipMismatch,
                Box::new(state),
            ));
        }
        if state.ui.as_ref().is_some_and(|ui| {
            ui.known_spells.len() > 4096
                || ui.component_templates.len() > 4096
                || bace_character::ui::validate(&ui.state).is_err()
        }) {
            return Err((
                CharacterRegistrationError::OwnershipMismatch,
                Box::new(state),
            ));
        }
        if state
            .native_services
            .as_ref()
            .is_some_and(|s| s.validate().is_err())
        {
            return Err((
                CharacterRegistrationError::OwnershipMismatch,
                Box::new(state),
            ));
        }
        let native_services = state.native_services;
        let contracts = state.contracts;
        let rare = state.rares;
        let ui = state.ui;
        match self.register(binding, state.progression) {
            Ok(()) => {
                let entry = self.entries.get_mut(&binding.actor).expect("registered");
                entry.native_services = native_services;
                entry.contracts = contracts;
                self.entries
                    .get_mut(&binding.actor)
                    .expect("registered actor")
                    .rare = rare;
                self.entries
                    .get_mut(&binding.actor)
                    .expect("registered actor")
                    .ui = ui;
                Ok(())
            }
            Err((error, progression)) => Err((
                error,
                Box::new(OwnedCharacterState {
                    progression,
                    native_services,
                    contracts,
                    rares: rare,
                    ui,
                }),
            )),
        }
    }
    pub(crate) fn take_complete(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<OwnedCharacterState, CharacterRegistrationError> {
        self.entries
            .get(&binding.actor)
            .filter(|e| e.binding == binding)
            .ok_or(CharacterRegistrationError::OwnershipMismatch)?;
        if self.reserved(binding.actor) {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        let entry = self
            .entries
            .remove(&binding.actor)
            .expect("validated binding");
        Ok(OwnedCharacterState {
            progression: entry.progression,
            native_services: entry.native_services,
            contracts: entry.contracts,
            rares: entry.rare,
            ui: entry.ui,
        })
    }
    pub(crate) fn prepare_native_rewards(
        &self,
        amounts: &[(EntityId, i64)],
        rare: Option<&RareDecision>,
    ) -> Option<Vec<(EntityId, ExperienceCredit)>> {
        let mut credits = self.prepare_rewards(amounts)?;
        if let Some(rare) = rare {
            let actor = EntityId(rare.character);
            let entry = self.entries.get(&actor)?;
            if entry.rare != Some(rare.previous) || entry.rare_pending.is_some() {
                return None;
            }
            if !credits.iter().any(|(id, _)| *id == actor) {
                credits.push((actor, entry.progression.propose_experience_credit(0).ok()?));
            }
            if rare.next != rare.previous {
                let (_, credit) = credits.iter_mut().find(|(id, _)| *id == actor)?;
                if credit.after_revision == credit.before_revision {
                    credit.after_revision = credit.before_revision.checked_add(1)?;
                }
            }
        }
        Some(credits)
    }
    pub(crate) fn reserve_native_rewards(
        &mut self,
        operation: u64,
        credits: &[(EntityId, ExperienceCredit)],
        rare: Option<&RareDecision>,
    ) {
        self.reserve_rewards(operation, credits);
        if let Some(rare) = rare {
            self.entries
                .get_mut(&EntityId(rare.character))
                .expect("prepared rare participant")
                .rare_pending = Some((operation, rare.clone()));
        }
    }
}
