//! AwardTrainingCredits joins both character counters to the source workflow.
use super::Kernel;
use crate::{NpcEffect, NpcProposal, npc::NpcTrainingCreditTicket};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation, NpcRewardKind};
impl Kernel {
    pub fn prepare_npc_training_credits(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<NpcTrainingCreditTicket, E> {
        self.npcs.validate_service(expected)?;
        if self
            .npcs
            .training_credit_services
            .contains_key(&expected.ticket)
        {
            return Err(E::Conflict);
        }
        let NpcEffect::Service(NpcOperation::Reward {
            kind: NpcRewardKind::TrainingCredits,
            amount,
            ..
        }) = expected.effect
        else {
            return Err(E::Unsupported);
        };
        let amount = i32::try_from(amount).map_err(|_| E::InvalidInput)?;
        let actor = expected.context.target.ok_or(E::MissingActor)?;
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.housing.reserved(actor)
            || self.npcs.reserved_except(actor, expected.ticket)
        {
            return Err(E::DurabilityPending);
        }
        self.prepare_inventory_time()
            .map_err(|_| E::DurabilityPending)?;
        let ticket = self
            .characters
            .prepare_npc_training_credits(expected, amount)
            .map_err(|_| E::InvalidInput)?;
        if self.magic.registry(actor).is_some() {
            self.magic
                .reserve_registry(actor, true, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.insert(expected.ticket);
        }
        self.npcs
            .training_credit_services
            .insert(expected.ticket, ticket.clone());
        Ok(ticket)
    }
    pub fn confirm_npc_training_credits_committed(
        &mut self,
        ticket: &NpcTrainingCreditTicket,
    ) -> Result<(), E> {
        if self.npcs.training_credit_services.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        self.npcs.validate_service(&ticket.npc)?;
        if self
            .characters
            .prepare_npc_training_credits(&ticket.npc, ticket.change.amount)
            .map_err(|_| E::Conflict)?
            != *ticket
        {
            return Err(E::Conflict);
        }
        if self.npcs.spellbook_registries.contains(&ticket.npc.ticket) {
            self.magic
                .reserve_registry(ticket.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.remove(&ticket.npc.ticket);
        }
        self.characters
            .adopt_npc_training_credits(ticket)
            .map_err(|_| E::Conflict)?;
        self.npcs
            .mark_service_adopted(&ticket.npc, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs
            .training_credit_services
            .remove(&ticket.npc.ticket);
        self.confirm_npc_committed(&ticket.npc)
    }
    pub fn reject_npc_training_credits(
        &mut self,
        ticket: &NpcTrainingCreditTicket,
    ) -> Result<(), E> {
        if self.npcs.training_credit_services.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        if self.npcs.spellbook_registries.contains(&ticket.npc.ticket) {
            self.magic
                .reserve_registry(ticket.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.remove(&ticket.npc.ticket);
        }
        self.npcs
            .training_credit_services
            .remove(&ticket.npc.ticket);
        Ok(())
    }
}
