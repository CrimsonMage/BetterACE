//! Immutable character capture; authoritative mutation remains in Characters.
use super::*;
#[derive(Debug)]
pub struct CharacterReadSnapshot {
    progression: bace_character::ProgressionSnapshot,
    rares: Option<bace_gameplay_api::CharacterRareState>,
    ui: Option<OwnedUiState>,
    native_services: Option<bace_character::CharacterServiceState>,
    contracts: Option<bace_quests::ContractRegistry>,
}
impl CharacterReadSnapshot {
    pub fn progression(&self) -> &CharacterProgression {
        self.progression.state()
    }
    pub fn rares(&self) -> Option<bace_gameplay_api::CharacterRareState> {
        self.rares
    }
    pub fn ui(&self) -> Option<&OwnedUiState> {
        self.ui.as_ref()
    }
    pub fn native_services(&self) -> Option<&bace_character::CharacterServiceState> {
        self.native_services.as_ref()
    }
    pub fn contracts(&self) -> Option<&bace_quests::ContractRegistry> {
        self.contracts.as_ref()
    }
}
impl Characters {
    pub(crate) fn binding_matches(&self, binding: CharacterBinding) -> bool {
        self.entries
            .get(&binding.actor)
            .is_some_and(|entry| entry.binding == binding)
    }
    pub(crate) fn read_snapshot(
        &self,
        binding: CharacterBinding,
    ) -> Result<CharacterReadSnapshot, CharacterRegistrationError> {
        self.can_take_complete(binding)?;
        self.capture_committed(binding)
    }
    pub(crate) fn read_operation_snapshot(
        &self,
        binding: CharacterBinding,
        operation: crate::PlayerSnapshotOperation,
        revision: u64,
    ) -> Result<CharacterReadSnapshot, CharacterRegistrationError> {
        let entry = self
            .entries
            .get(&binding.actor)
            .filter(|e| e.binding == binding)
            .ok_or(CharacterRegistrationError::OwnershipMismatch)?;
        let owned = match operation {
            crate::PlayerSnapshotOperation::StaffGag(id) => id != 0 && entry.gag == Some(id),
            crate::PlayerSnapshotOperation::Portal(id) => id != 0 && entry.portal == Some(id),
            // NPC reservation ownership belongs to Npcs. Kernel validates the
            // exact ticket/participant/revision before invoking this projection.
            crate::PlayerSnapshotOperation::Npc { ticket } => ticket != 0,
            crate::PlayerSnapshotOperation::NpcHandIn { operation } => operation != 0,
            crate::PlayerSnapshotOperation::Crafting(operation) => {
                operation != 0 && entry.crafting == Some(operation)
            }
            // The inventory owner validates the exact ticket in Kernel.
            crate::PlayerSnapshotOperation::Inventory(operation) => {
                operation != 0 && entry.equipment.is_none_or(|id| id == operation)
            }
            // Kernel verifies the exact pending pet ticket and inventory hold.
            crate::PlayerSnapshotOperation::Pet(operation) => {
                operation != 0 && entry.equipment.is_none()
            }
            crate::PlayerSnapshotOperation::PhysicalAmmo(operation) => {
                operation != 0 && entry.equipment.is_none()
            }
            crate::PlayerSnapshotOperation::Skill(id) => {
                id != 0 && self.pending_skill_operation(binding.actor) == Some(id)
            }
            crate::PlayerSnapshotOperation::AttributeTransfer(id) => {
                id != 0 && self.pending_attribute_transfer_operation(binding.actor) == Some(id)
            }
            crate::PlayerSnapshotOperation::Allegiance(id) => {
                id != 0 && self.social_reward_operation(binding.actor) == Some(id)
            }
            crate::PlayerSnapshotOperation::PlayerDeath(id) => id != 0 && entry.death == Some(id),
            crate::PlayerSnapshotOperation::PveDeath(id) => {
                id != 0
                    && entry.reward.is_some_and(|(operation, credit)| {
                        operation == id && credit.before_revision == revision
                    })
            }
            crate::PlayerSnapshotOperation::StaffSpell(id) => {
                id != 0
                    && entry
                        .staff_spell
                        .as_ref()
                        .is_some_and(|t| t.operation == id)
            }
        };
        if !owned || entry.progression.revision() != revision {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        self.capture_committed(binding)
    }
    fn capture_committed(
        &self,
        binding: CharacterBinding,
    ) -> Result<CharacterReadSnapshot, CharacterRegistrationError> {
        let entry = self
            .entries
            .get(&binding.actor)
            .ok_or(CharacterRegistrationError::OwnershipMismatch)?;
        Ok(CharacterReadSnapshot {
            progression: entry.progression.read_snapshot(),
            rares: entry.rare,
            ui: entry.ui.as_ref().map(|ui| OwnedUiState {
                state: ui.state.clone(),
                known_spells: ui.known_spells.clone(),
                component_templates: ui.component_templates.clone(),
                entered: ui.entered,
            }),
            native_services: entry.native_services.clone(),
            contracts: entry.contracts.clone(),
        })
    }
}
