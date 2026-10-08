use super::*;
use bace_character::{CharacterServiceChange, CharacterServiceState};
impl Characters {
    pub(crate) fn adopt_npc_skill_reset(
        &mut self,
        actor: EntityId,
        proposal: bace_character::SkillProposal,
    ) -> Result<(), Box<bace_character::StaleSkillProposal>> {
        let Some(entry) = self.entries.get_mut(&actor) else {
            return Err(Box::new(bace_character::StaleSkillProposal(proposal)));
        };
        entry.progression.adopt_skill_proposal(proposal).map(|_| ())
    }
    pub(crate) fn prepare_npc_training_credits(
        &self,
        proposal: &crate::NpcProposal,
        amount: i32,
    ) -> Result<crate::npc::NpcTrainingCreditTicket, ()> {
        let actor = proposal.context.target.ok_or(())?;
        let entry = self.entries.get(&actor).ok_or(())?;
        let change = entry
            .progression
            .propose_training_credits(entry.native_services.as_ref().ok_or(())?, amount)
            .map_err(|_| ())?;
        Ok(crate::npc::NpcTrainingCreditTicket {
            npc: proposal.clone(),
            actor,
            change,
        })
    }
    pub(crate) fn adopt_npc_training_credits(
        &mut self,
        ticket: &crate::npc::NpcTrainingCreditTicket,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&ticket.actor).ok_or(())?;
        entry
            .progression
            .adopt_training_credits(entry.native_services.as_mut().ok_or(())?, &ticket.change)
            .map_err(|_| ())
    }
    pub(crate) fn prepare_npc_spellbook(
        &self,
        proposal: &crate::NpcProposal,
        spell: u32,
    ) -> Result<crate::npc::NpcSpellbookTicket, ()> {
        let actor = proposal.context.target.ok_or(())?;
        let entry = self.entries.get(&actor).ok_or(())?;
        let before = entry.ui.as_ref().ok_or(())?.known_spells.clone();
        let mut after = before.clone();
        if !after.contains(&spell) {
            if after.len() >= 4096 {
                return Err(());
            }
            after.push(spell);
        }
        let before_revision = entry.progression.revision();
        let after_revision = before_revision
            .checked_add(u64::from(before != after))
            .ok_or(())?;
        Ok(crate::npc::NpcSpellbookTicket {
            npc: proposal.clone(),
            actor,
            spell,
            before_revision,
            after_revision,
            before,
            after,
        })
    }
    pub(crate) fn adopt_npc_spellbook(
        &mut self,
        ticket: &crate::npc::NpcSpellbookTicket,
    ) -> Result<(), ()> {
        let expected = self.prepare_npc_spellbook(&ticket.npc, ticket.spell)?;
        if &expected != ticket {
            return Err(());
        }
        let entry = self.entries.get_mut(&ticket.actor).ok_or(())?;
        if ticket.after_revision != ticket.before_revision {
            entry.progression.touch_revision().map_err(|_| ())?;
        }
        entry.ui.as_mut().ok_or(())?.known_spells = ticket.after.clone();
        Ok(())
    }
    pub(crate) fn native_services(&self, actor: EntityId) -> Option<&CharacterServiceState> {
        self.entries.get(&actor)?.native_services.as_ref()
    }
    pub(crate) fn contracts(&self, actor: EntityId) -> Option<&bace_quests::ContractRegistry> {
        self.entries.get(&actor)?.contracts.as_ref()
    }
    pub(crate) fn register_native_services(
        &mut self,
        actor: EntityId,
        state: CharacterServiceState,
        contracts: bace_quests::ContractRegistry,
    ) -> Result<(), ()> {
        state.validate().map_err(|_| ())?;
        let e = self.entries.get_mut(&actor).ok_or(())?;
        if e.native_services.is_some() || e.contracts.is_some() {
            return Err(());
        }
        e.native_services = Some(state);
        e.contracts = Some(contracts);
        Ok(())
    }
    pub(crate) fn adopt_native_services(
        &mut self,
        actor: EntityId,
        change: CharacterServiceChange,
    ) -> Result<(), ()> {
        let e = self.entries.get_mut(&actor).ok_or(())?;
        if e.progression.revision() != change.before_revision
            || e.native_services.as_ref() != Some(&change.before)
        {
            return Err(());
        }
        change.after.validate().map_err(|_| ())?;
        let changed = change.before != change.after;
        if change.after_revision
            != change
                .before_revision
                .checked_add(u64::from(changed))
                .ok_or(())?
        {
            return Err(());
        }
        if changed {
            e.progression.touch_revision().map_err(|_| ())?;
        }
        e.native_services = Some(change.after);
        Ok(())
    }
    pub(crate) fn adopt_contract(
        &mut self,
        actor: EntityId,
        revision: u64,
        change: bace_quests::ContractChange,
    ) -> Result<(), ()> {
        let e = self.entries.get_mut(&actor).ok_or(())?;
        if e.progression.revision() != revision
            || e.contracts.as_ref().ok_or(())?.entries() != change.before
        {
            return Err(());
        }
        let changed = change.before != change.after;
        if changed && revision == u64::MAX {
            return Err(());
        }
        e.contracts
            .as_mut()
            .ok_or(())?
            .adopt(change)
            .map_err(|_| ())?;
        if changed {
            e.progression.touch_revision().map_err(|_| ())?;
        }
        Ok(())
    }
}
impl Characters {
    pub(crate) fn earned_experience(
        &self,
        actor: EntityId,
        table: &bace_character::CharacterLevelTable,
        amount: u64,
    ) -> Result<bace_character::EarnedExperienceChange, ()> {
        let e = self.entries.get(&actor).ok_or(())?;
        e.progression
            .propose_earned_experience(e.native_services.as_ref().ok_or(())?, table, amount)
            .map_err(|_| ())
    }
    pub(crate) fn adopt_earned_experience(
        &mut self,
        actor: EntityId,
        change: bace_character::EarnedExperienceChange,
    ) -> Result<(), ()> {
        let e = self.entries.get_mut(&actor).ok_or(())?;
        e.progression
            .adopt_earned_experience(e.native_services.as_mut().ok_or(())?, change)
            .map_err(|_| ())
    }
}

impl Characters {
    pub(crate) fn validate_npc_aggregate(
        &self,
        actor: EntityId,
        fence: crate::npc::NpcAggregateFence,
    ) -> Result<(), ()> {
        let e = self.entries.get(&actor).ok_or(())?;
        if e.progression.revision() != fence.before_revision
            || fence.after_revision < fence.before_revision
            || fence.after_revision - fence.before_revision > 1
        {
            return Err(());
        }
        Ok(())
    }
    pub(crate) fn adopt_npc_aggregate(
        &mut self,
        actor: EntityId,
        fence: crate::npc::NpcAggregateFence,
    ) -> Result<(), ()> {
        self.validate_npc_aggregate(actor, fence)?;
        if fence.after_revision != fence.before_revision {
            self.entries
                .get_mut(&actor)
                .ok_or(())?
                .progression
                .touch_revision()
                .map_err(|_| ())?;
        }
        Ok(())
    }
}

impl Characters {
    pub(crate) fn knows_spell(&self, actor: EntityId, spell: u32) -> Option<bool> {
        self.entries
            .get(&actor)?
            .ui
            .as_ref()
            .map(|ui| ui.known_spells.contains(&spell))
    }
}
