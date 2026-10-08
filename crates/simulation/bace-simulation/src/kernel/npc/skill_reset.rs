//! UntrainSkill delegates to the character ResetSkill proposal and derived
//! profile validation, without forging a player action sequence.
use super::Kernel;
use crate::{NpcEffect, NpcProposal, npc::NpcSkillResetTicket};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation, NpcRewardKind};
impl Kernel {
    pub fn prepare_npc_skill_reset(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<NpcSkillResetTicket, E> {
        self.npcs.validate_service(expected)?;
        if self.npcs.skill_resets.contains_key(&expected.ticket) {
            return Err(E::Conflict);
        }
        let NpcEffect::Service(NpcOperation::Reward {
            kind: NpcRewardKind::UntrainSkill,
            stat: Some(skill),
            ..
        }) = expected.effect
        else {
            return Err(E::Unsupported);
        };
        let actor = expected.context.target.ok_or(E::MissingActor)?;
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.housing.reserved(actor)
            || self.magic.busy(actor)
            || self.npcs.reserved_except(actor, expected.ticket)
        {
            return Err(E::DurabilityPending);
        }
        self.prepare_inventory_time()
            .map_err(|_| E::DurabilityPending)?;
        let proposal = self
            .characters
            .get(actor)
            .ok_or(E::MissingActor)?
            .propose_reset_skill(skill)
            .map_err(|_| E::InvalidInput)?;
        self.validate_proposed_skill_refresh(actor, proposal.proposed_state())
            .map_err(|_| E::Conflict)?;
        let ticket = NpcSkillResetTicket {
            npc: expected.clone(),
            actor,
            change: proposal.change(),
            before_revision: proposal.expected_revision(),
        };
        if self.magic.registry(actor).is_some() {
            self.magic
                .reserve_registry(actor, true, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.insert(expected.ticket);
        }
        self.npcs
            .skill_resets
            .insert(expected.ticket, (ticket.clone(), Some(proposal)));
        Ok(ticket)
    }
    pub fn confirm_npc_skill_reset_committed(
        &mut self,
        ticket: &NpcSkillResetTicket,
    ) -> Result<(), E> {
        let (held, proposal) = self
            .npcs
            .skill_resets
            .get(&ticket.npc.ticket)
            .ok_or(E::Conflict)?;
        if held != ticket {
            return Err(E::Conflict);
        }
        self.npcs.validate_service(&ticket.npc)?;
        if let Some(proposal) = proposal {
            self.validate_proposed_skill_refresh(ticket.actor, proposal.proposed_state())
                .map_err(|_| E::Conflict)?;
            let proposal = self
                .npcs
                .skill_resets
                .get_mut(&ticket.npc.ticket)
                .and_then(|(_, p)| p.take())
                .ok_or(E::Conflict)?;
            if let Err(stale) = self
                .characters
                .adopt_npc_skill_reset(ticket.actor, proposal)
            {
                self.npcs
                    .skill_resets
                    .get_mut(&ticket.npc.ticket)
                    .ok_or(E::Conflict)?
                    .1 = Some(stale.0);
                return Err(E::Conflict);
            }
        }
        self.refresh_character_skills(ticket.actor)
            .map_err(|_| E::Conflict)?;
        if self.npcs.spellbook_registries.contains(&ticket.npc.ticket) {
            self.magic
                .reserve_registry(ticket.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.remove(&ticket.npc.ticket);
        }
        self.npcs
            .mark_service_adopted(&ticket.npc, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.skill_resets.remove(&ticket.npc.ticket);
        self.confirm_npc_committed(&ticket.npc)
    }
    pub fn reject_npc_skill_reset(&mut self, ticket: &NpcSkillResetTicket) -> Result<(), E> {
        if self
            .npcs
            .skill_resets
            .get(&ticket.npc.ticket)
            .is_none_or(|(t, p)| t != ticket || p.is_none())
        {
            return Err(E::Conflict);
        }
        if self.npcs.spellbook_registries.contains(&ticket.npc.ticket) {
            self.magic
                .reserve_registry(ticket.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.remove(&ticket.npc.ticket);
        }
        self.npcs.skill_resets.remove(&ticket.npc.ticket);
        Ok(())
    }
}
