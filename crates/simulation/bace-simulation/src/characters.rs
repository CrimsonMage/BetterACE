mod attribute_transfer;
mod crafting;
use attribute_transfer::PendingAttributeTransfer;
pub use attribute_transfer::{AttributeTransferActionError, AttributeTransferTicket};
mod equipment;
mod portals;
mod read_snapshot;
mod staff_spells;
pub use read_snapshot::CharacterReadSnapshot;
mod admission;
mod death;
mod gags;
mod npc_services;
mod skills;
mod social_rewards;
use skills::PendingSkill;
pub use skills::{SkillActionError, SkillIntent, SkillTicket};
mod ui;
pub use ui::OwnedUiState;
mod rares;
pub use rares::OwnedCharacterState;
use std::collections::BTreeMap;

use bace_character::{CharacterProgression, ExperienceCredit};
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
    DurabilityPending,
    CompleteStateRequired,
}

struct OwnedCharacter {
    binding: CharacterBinding,
    native_services: Option<bace_character::CharacterServiceState>,
    contracts: Option<bace_quests::ContractRegistry>,
    ui: Option<OwnedUiState>,
    skill: Option<PendingSkill>,
    attribute_transfer: Option<PendingAttributeTransfer>,
    progression: CharacterProgression,
    last_sequence: Option<u32>,
    reward: Option<(u64, ExperienceCredit)>,
    social_reward: Option<u64>,
    gag: Option<u64>,
    portal: Option<u64>,
    staff_spell: Option<bace_gameplay_api::staff::StaffSpellTicket>,
    death: Option<u64>,
    crafting: Option<u64>,
    equipment: Option<u64>,
    rare: Option<bace_gameplay_api::CharacterRareState>,
    rare_pending: Option<(u64, bace_gameplay_api::RareDecision)>,
}

/// Character state lives on the same Kernel owner as world/physics; no Body or
/// accepted pose is copied here. Registration work is bounded by capacity.
pub(crate) struct Characters {
    entries: BTreeMap<EntityId, OwnedCharacter>,
    capacity: usize,
    next_skill: u64,
    next_attribute_transfer: u64,
}

impl Characters {
    pub(crate) fn adopt_crafting_revision(
        &mut self,
        actor: EntityId,
        before: u64,
        after: u64,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.progression.revision() != before
            || ((entry.portal.is_some() || entry.staff_spell.is_some())
                || entry.reward.is_some()
                || (entry.social_reward.is_some() || entry.gag.is_some())
                || (entry.death.is_some()
                    || (entry.crafting.is_some() || entry.equipment.is_some())))
            || entry.rare_pending.is_some()
            || entry.skill.is_some()
            || entry.attribute_transfer.is_some()
        {
            return Err(());
        }
        if before == after {
            return Ok(());
        }
        if before.checked_add(1) != Some(after) {
            return Err(());
        }
        entry
            .progression
            .touch_revision()
            .map(|_| ())
            .map_err(|_| ())
    }
    pub(crate) fn touch_auxiliary(&mut self, actor: EntityId) -> Result<bool, ()> {
        if self.reserved(actor) {
            return Ok(false);
        }
        let Some(entry) = self.entries.get_mut(&actor) else {
            return Ok(false);
        };
        entry.progression.touch_revision().map_err(|_| ())?;
        Ok(true)
    }
    pub(crate) fn can_take_complete(
        &self,
        binding: CharacterBinding,
    ) -> Result<(), CharacterRegistrationError> {
        let entry = self
            .entries
            .get(&binding.actor)
            .filter(|e| e.binding == binding)
            .ok_or(CharacterRegistrationError::OwnershipMismatch)?;
        if ((entry.portal.is_some() || entry.staff_spell.is_some())
            || entry.reward.is_some()
            || (entry.social_reward.is_some() || entry.gag.is_some())
            || (entry.death.is_some() || (entry.crafting.is_some() || entry.equipment.is_some())))
            || entry.rare_pending.is_some()
            || entry.skill.is_some()
            || entry.attribute_transfer.is_some()
        {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        Ok(())
    }
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity,
            next_skill: 0,
            next_attribute_transfer: 0,
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
                native_services: None,
                contracts: None,
                ui: None,
                skill: None,
                attribute_transfer: None,
                progression,
                last_sequence: None,
                reward: None,
                social_reward: None,
                gag: None,
                portal: None,
                staff_spell: None,
                death: None,
                crafting: None,
                equipment: None,
                rare: None,
                rare_pending: None,
            },
        );
        Ok(())
    }

    pub(crate) fn take(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<CharacterProgression, CharacterRegistrationError> {
        if self.entries.get(&binding.actor).is_some_and(|entry| {
            entry.binding == binding
                && (((entry.portal.is_some() || entry.staff_spell.is_some())
                    || entry.reward.is_some()
                    || (entry.social_reward.is_some() || entry.gag.is_some())
                    || (entry.death.is_some()
                        || (entry.crafting.is_some() || entry.equipment.is_some())))
                    || entry.rare_pending.is_some()
                    || entry.skill.is_some())
                || entry.attribute_transfer.is_some()
        }) {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        if self
            .entries
            .get(&binding.actor)
            .is_none_or(|entry| entry.binding != binding)
        {
            return Err(CharacterRegistrationError::OwnershipMismatch);
        }
        if self.entries.get(&binding.actor).is_some_and(|e| {
            e.rare.is_some()
                || e.ui.is_some()
                || e.native_services.is_some()
                || e.contracts.is_some()
        }) {
            return Err(CharacterRegistrationError::CompleteStateRequired);
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

    pub(crate) fn adopt_script_experience(
        &mut self,
        actor: EntityId,
        credit: ExperienceCredit,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.attribute_transfer.is_some()
            || (entry.portal.is_some() || entry.staff_spell.is_some())
            || entry.reward.is_some()
            || (entry.social_reward.is_some() || entry.gag.is_some())
            || (entry.death.is_some() || (entry.crafting.is_some() || entry.equipment.is_some()))
        {
            return Err(());
        }
        entry
            .progression
            .adopt_experience_credit(credit)
            .map_err(|_| ())
    }
    pub(crate) fn adopt_script_luminance(
        &mut self,
        actor: EntityId,
        credit: bace_character::LuminanceCredit,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.attribute_transfer.is_some()
            || (entry.portal.is_some() || entry.staff_spell.is_some())
            || entry.reward.is_some()
            || (entry.social_reward.is_some() || entry.gag.is_some())
            || (entry.death.is_some() || (entry.crafting.is_some() || entry.equipment.is_some()))
        {
            return Err(());
        }
        entry.progression.adopt_luminance(credit).map_err(|_| ())
    }
    pub(crate) fn reserved(&self, actor: EntityId) -> bool {
        self.entries.get(&actor).is_some_and(|e| {
            ((e.portal.is_some() || e.staff_spell.is_some())
                || e.reward.is_some()
                || (e.social_reward.is_some() || e.gag.is_some())
                || (e.death.is_some() || (e.crafting.is_some() || e.equipment.is_some())))
                || e.rare_pending.is_some()
                || e.skill.is_some()
                || e.attribute_transfer.is_some()
        })
    }
    pub(crate) fn prepare_rewards(
        &self,
        amounts: &[(EntityId, i64)],
    ) -> Option<Vec<(EntityId, ExperienceCredit)>> {
        amounts
            .iter()
            .map(|(actor, amount)| {
                let entry = self.entries.get(actor)?;
                if ((entry.portal.is_some() || entry.staff_spell.is_some())
                    || entry.reward.is_some()
                    || (entry.social_reward.is_some() || entry.gag.is_some())
                    || (entry.death.is_some()
                        || (entry.crafting.is_some() || entry.equipment.is_some())))
                    || entry.rare_pending.is_some()
                    || entry.skill.is_some()
                    || entry.attribute_transfer.is_some()
                {
                    return None;
                }
                let credit = entry
                    .progression
                    .propose_experience_credit(u64::try_from(*amount).ok()?)
                    .ok()?;
                Some((*actor, credit))
            })
            .collect()
    }
    pub(crate) fn reserve_rewards(
        &mut self,
        operation: u64,
        credits: &[(EntityId, ExperienceCredit)],
    ) {
        for (actor, credit) in credits {
            self.entries
                .get_mut(actor)
                .expect("validated reward participant")
                .reward = Some((operation, *credit));
        }
    }
    pub(crate) fn commit_rewards(&mut self, operation: u64) -> Result<(), ()> {
        for entry in self.entries.values() {
            if let Some((id, credit)) = entry.reward
                && id == operation
                && (entry.progression.revision() != credit.before_revision
                    || entry.progression.available_experience() != credit.before_available)
            {
                return Err(());
            }
        }
        for entry in self.entries.values() {
            if let Some((id, rare)) = &entry.rare_pending
                && *id == operation
                && (entry.rare != Some(rare.previous)
                    || entry.reward.is_none_or(|(op, _)| op != operation))
            {
                return Err(());
            }
        }
        for entry in self.entries.values_mut() {
            if let Some((id, credit)) = entry.reward
                && id == operation
            {
                entry
                    .progression
                    .adopt_combined_reward(
                        credit,
                        entry
                            .rare_pending
                            .as_ref()
                            .is_some_and(|(op, r)| *op == operation && r.next != r.previous),
                    )
                    .map_err(|_| ())?;
                entry.reward = None;
                if let Some((op, rare)) = &entry.rare_pending
                    && *op == operation
                {
                    entry.rare = Some(rare.next);
                    entry.rare_pending = None;
                }
            }
        }
        Ok(())
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
        self.authorize(context, actor_exists)?;
        self.entries
            .get_mut(&context.actor)
            .expect("authorized entry")
            .progression
            .raise(request)
            .map_err(ProgressionActionRejection::Domain)
    }

    pub(crate) fn authorize(
        &mut self,
        context: ActionContext,
        actor_exists: bool,
    ) -> Result<(), ProgressionActionRejection> {
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
        Ok(())
    }
}

pub use skills::{SkillOutcome, SkillStage, UiOutcome};

pub use ui::OwnedPlayerState;
