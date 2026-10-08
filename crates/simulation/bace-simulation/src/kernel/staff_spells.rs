//! Receipt-gated addspell/removespell handlers; prepared names come from DAT.
use super::*;
use bace_gameplay_api::staff::{
    StaffError as E, StaffEvent, StaffSpellDefinition, StaffSpellTicket,
};
impl Kernel {
    pub fn register_staff_spell_definitions(
        &mut self,
        definitions: Vec<StaffSpellDefinition>,
    ) -> Result<(), E> {
        if definitions.len() > 8192 || !self.staff.spells.is_empty() {
            return Err(E::Capacity);
        }
        let mut values = std::collections::BTreeMap::new();
        for value in definitions {
            if value.spell == 0
                || value.spell > 65535
                || value.name.len() > 1024
                || value.enum_name.len() > 128
                || values.insert(value.spell, value).is_some()
            {
                return Err(E::Invalid);
            }
        }
        self.staff.spells = values;
        Ok(())
    }
    pub fn prepare_staff_spellbook(
        &mut self,
        context: ActionContext,
        operation: u64,
        spell: u32,
        learn: bool,
        sudo: bool,
    ) -> Result<StaffSpellTicket, E> {
        if operation == 0 {
            return Err(E::Invalid);
        }
        let definition = self.staff.spells.get(&spell).ok_or(E::Invalid)?.clone();
        self.authorize_staff(context, 4, sudo)?;
        let actor = context.actor;
        let now = self.tick as f64 / 30.;
        if self.staff.spell_registries.contains_key(&operation)
            || self.magic.registry_reserved(actor)
        {
            return Err(E::Busy);
        }
        self.magic.prepare_registry_time(now).map_err(|_| E::Busy)?;
        self.sync_registry_revisions().map_err(|_| E::Overflow)?;
        let reserved = self.magic.registry(actor).is_some();
        if reserved {
            self.magic
                .reserve_registry(actor, true, now)
                .map_err(|_| E::Busy)?;
        }
        let ticket = match self.characters.prepare_staff_spell(
            context,
            operation,
            spell,
            definition.name,
            learn,
        ) {
            Ok(ticket) => ticket,
            Err(error) => {
                if reserved {
                    self.magic
                        .reserve_registry(actor, false, now)
                        .expect("same-time registry release");
                }
                return Err(error);
            }
        };
        if reserved {
            self.staff.spell_registries.insert(operation, actor);
        }

        self.staff.push(StaffEvent::SpellProposal(ticket.clone()));
        Ok(ticket)
    }
    pub fn confirm_staff_spellbook(&mut self, ticket: &StaffSpellTicket) -> Result<(), E> {
        if !self.staff.room(1) {
            return Err(E::Capacity);
        }
        self.characters.validate_staff_spell(ticket)?;
        if let Some(&actor) = self.staff.spell_registries.get(&ticket.operation) {
            if actor != ticket.context.actor {
                return Err(E::Stale);
            }
            self.magic
                .reserve_registry(actor, false, self.tick as f64 / 30.)
                .map_err(|_| E::Busy)?;
            self.staff.spell_registries.remove(&ticket.operation);
        }
        self.characters.adopt_staff_spell(ticket)?;
        self.magic
            .staff_known_spell(ticket.context.actor, ticket.spell, ticket.learn);
        self.staff.push(StaffEvent::Spellbook {
            context: ticket.context,
            spell: ticket.spell,
            name: ticket.name.clone(),
            learn: ticket.learn,
            changed: ticket.before != ticket.after,
            revision: ticket.after_revision,
        });
        Ok(())
    }
    pub fn reject_staff_spellbook(&mut self, ticket: &StaffSpellTicket) -> Result<(), E> {
        self.characters.validate_staff_spell(ticket)?;
        if let Some(&actor) = self.staff.spell_registries.get(&ticket.operation) {
            if actor != ticket.context.actor {
                return Err(E::Stale);
            }
            self.magic
                .reserve_registry(actor, false, self.tick as f64 / 30.)
                .map_err(|_| E::Busy)?;
            self.staff.spell_registries.remove(&ticket.operation);
        }
        self.characters.reject_staff_spell(ticket)
    }
}
