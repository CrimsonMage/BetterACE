//! TeachSpell changes the canonical character spellbook only after the joint
//! player/workflow receipt. Magic permissions derive from that accepted owner.
use super::Kernel;
use crate::{NpcEffect, NpcProposal, npc::NpcSpellbookTicket};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation, NpcRewardKind};
impl Kernel {
    pub fn register_npc_spell_catalog(&mut self, spells: Vec<u32>) -> Result<(), E> {
        if self.npcs.spell_catalog.is_some()
            || spells.len() > 8192
            || spells.iter().any(|s| *s == 0 || *s > 65535)
        {
            return Err(E::InvalidInput);
        }
        let count = spells.len();
        let catalog = spells
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if catalog.len() != count {
            return Err(E::InvalidInput);
        }
        self.npcs.spell_catalog = Some(catalog);
        Ok(())
    }
    pub fn prepare_npc_spellbook(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<NpcSpellbookTicket, E> {
        self.npcs.validate_service(expected)?;
        if self.npcs.spellbook_services.contains_key(&expected.ticket) {
            return Err(E::Conflict);
        }
        let NpcEffect::Service(NpcOperation::Reward {
            kind: NpcRewardKind::TeachSpell,
            amount,
            ..
        }) = expected.effect
        else {
            return Err(E::Unsupported);
        };
        let spell = u32::try_from(amount).map_err(|_| E::InvalidInput)?;
        if !self
            .npcs
            .spell_catalog
            .as_ref()
            .is_some_and(|s| s.contains(&spell))
        {
            return Err(E::MissingContent);
        }
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
            .prepare_npc_spellbook(expected, spell)
            .map_err(|_| E::Conflict)?;
        if self.magic.registry(actor).is_some() {
            self.magic
                .reserve_registry(actor, true, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.insert(expected.ticket);
        }
        self.npcs
            .spellbook_services
            .insert(expected.ticket, ticket.clone());
        Ok(ticket)
    }
    pub fn confirm_npc_spellbook_committed(
        &mut self,
        ticket: &NpcSpellbookTicket,
    ) -> Result<(), E> {
        if self.npcs.spellbook_services.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        self.npcs.validate_service(&ticket.npc)?;
        if self
            .characters
            .prepare_npc_spellbook(&ticket.npc, ticket.spell)
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
            .adopt_npc_spellbook(ticket)
            .map_err(|_| E::Conflict)?;
        self.magic.adopt_known_spell(ticket.actor, ticket.spell);
        self.npcs
            .mark_service_adopted(&ticket.npc, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.spellbook_services.remove(&ticket.npc.ticket);
        self.confirm_npc_committed(&ticket.npc)
    }
    pub fn reject_npc_spellbook(&mut self, ticket: &NpcSpellbookTicket) -> Result<(), E> {
        if self.npcs.spellbook_services.get(&ticket.npc.ticket) != Some(ticket) {
            return Err(E::Conflict);
        }
        if self.npcs.spellbook_registries.contains(&ticket.npc.ticket) {
            self.magic
                .reserve_registry(ticket.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| E::DurabilityPending)?;
            self.npcs.spellbook_registries.remove(&ticket.npc.ticket);
        }
        self.npcs.spellbook_services.remove(&ticket.npc.ticket);
        Ok(())
    }
}
