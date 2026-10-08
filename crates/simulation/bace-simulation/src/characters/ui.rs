use super::*;
use bace_gameplay_api::{CharacterUi, UiError, UiRequest};
#[derive(Debug)]
pub struct OwnedUiState {
    pub state: CharacterUi,
    pub known_spells: Vec<u32>,
    pub component_templates: Vec<u32>,
    pub entered: bool,
}
impl Characters {
    pub(crate) fn register_ui(
        &mut self,
        binding: CharacterBinding,
        state: OwnedUiState,
    ) -> Result<(), (UiError, Box<OwnedUiState>)> {
        if state.known_spells.len() > 4096 || state.component_templates.len() > 4096 {
            return Err((UiError::Capacity, Box::new(state)));
        }
        if bace_character::ui::validate(&state.state).is_err() {
            return Err((UiError::Invalid, Box::new(state)));
        }
        let Some(entry) = self
            .entries
            .get_mut(&binding.actor)
            .filter(|e| e.binding == binding && e.ui.is_none())
        else {
            return Err((UiError::Ownership, Box::new(state)));
        };
        entry.ui = Some(state);
        Ok(())
    }
    pub(crate) fn ui(&self, actor: EntityId) -> Option<&CharacterUi> {
        self.entries.get(&actor)?.ui.as_ref().map(|v| &v.state)
    }
    pub(crate) fn entered_binding(&self, actor: EntityId) -> Option<CharacterBinding> {
        let entry = self.entries.get(&actor)?;
        entry
            .ui
            .as_ref()
            .is_some_and(|ui| ui.entered)
            .then_some(entry.binding)
    }
    pub(crate) fn entered(&self, actor: EntityId) -> bool {
        self.entries
            .get(&actor)
            .and_then(|entry| entry.ui.as_ref())
            .is_some_and(|ui| ui.entered)
    }
    pub(crate) fn enter_ui(&mut self, binding: CharacterBinding) -> Result<(), UiError> {
        let entry = self
            .entries
            .get_mut(&binding.actor)
            .filter(|e| e.binding == binding)
            .ok_or(UiError::Ownership)?;
        entry.ui.as_mut().ok_or(UiError::Invalid)?.entered = true;
        Ok(())
    }
    pub(crate) fn apply_ui(
        &mut self,
        context: ActionContext,
        request: UiRequest,
        actor_exists: bool,
    ) -> Result<u64, UiError> {
        if self.reserved(context.actor) {
            return Err(UiError::DurabilityPending);
        }
        self.authorize(context, actor_exists)
            .map_err(|_| UiError::Ownership)?;
        let entry = self
            .entries
            .get_mut(&context.actor)
            .ok_or(UiError::Ownership)?;
        if entry.progression.revision() == u64::MAX {
            return Err(UiError::RevisionExhausted);
        }
        let ui = entry.ui.as_mut().ok_or(UiError::Invalid)?;
        if bace_character::ui::apply(
            &mut ui.state,
            request,
            ui.entered,
            &ui.known_spells,
            &ui.component_templates,
        )? {
            entry
                .progression
                .touch_revision()
                .map_err(|_| UiError::RevisionExhausted)?;
        }
        Ok(entry.progression.revision())
    }
}

#[derive(Debug)]
pub struct OwnedPlayerState {
    pub gag: Option<crate::GagRecovery>,
    pub equipment_mana: Option<crate::EquipmentManaRecovery>,
    pub chat_age: Option<u64>,
    pub physical_recovery: f64,
    pub death: Option<crate::PlayerDeathState>,
    pub social: Option<bace_social::SocialPreferences>,
    pub portal_links: Option<bace_interactions::PortalLinks>,
    pub world: Option<crate::PlayerWorldSnapshot>,
    pub recovery: Option<bace_magic::CastRecovery>,
    pub character: super::OwnedCharacterState,
    pub enchantments: Option<bace_magic::EnchantmentRegistry>,
    pub item_experience: Vec<crate::PreparedItemExperience>,
    pub item_enchantments: Vec<(EntityId, bace_magic::EnchantmentRegistry)>,
}
