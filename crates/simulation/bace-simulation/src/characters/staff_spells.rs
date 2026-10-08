//! Staff spellbook writes reserve the same character aggregate as skill/XP work.
use super::*;
use bace_gameplay_api::staff::{StaffError as E, StaffSpellTicket};
impl Characters {
    pub(crate) fn prepare_staff_spell(
        &mut self,
        context: ActionContext,
        operation: u64,
        spell: u32,
        name: String,
        learn: bool,
    ) -> Result<StaffSpellTicket, E> {
        if self.reserved(context.actor) {
            return Err(E::Busy);
        }
        let entry = self.entries.get_mut(&context.actor).ok_or(E::NotBound)?;
        let before = entry.ui.as_ref().ok_or(E::Invalid)?.known_spells.clone();
        let mut after = before.clone();
        if learn {
            if !after.contains(&spell) {
                if after.len() >= 4096 {
                    return Err(E::Capacity);
                }
                after.push(spell);
            }
        } else {
            after.retain(|id| *id != spell);
        }
        let before_revision = entry.progression.revision();
        let after_revision = if before == after {
            before_revision
        } else {
            before_revision.checked_add(1).ok_or(E::Overflow)?
        };
        let ticket = StaffSpellTicket {
            operation,
            context,
            spell,
            name,
            learn,
            before_revision,
            after_revision,
            before,
            after,
        };
        entry.staff_spell = Some(ticket.clone());
        Ok(ticket)
    }
    pub(crate) fn validate_staff_spell(&self, ticket: &StaffSpellTicket) -> Result<(), E> {
        let entry = self.entries.get(&ticket.context.actor).ok_or(E::NotBound)?;
        if entry.staff_spell.as_ref() != Some(ticket)
            || entry.progression.revision() != ticket.before_revision
            || entry
                .ui
                .as_ref()
                .is_none_or(|ui| ui.known_spells != ticket.before)
        {
            return Err(E::Stale);
        }
        Ok(())
    }
    pub(crate) fn adopt_staff_spell(&mut self, ticket: &StaffSpellTicket) -> Result<(), E> {
        self.validate_staff_spell(ticket)?;
        let entry = self
            .entries
            .get_mut(&ticket.context.actor)
            .ok_or(E::NotBound)?;
        if ticket.before_revision != ticket.after_revision {
            entry
                .progression
                .touch_revision()
                .map_err(|_| E::Overflow)?;
        }
        entry.ui.as_mut().ok_or(E::Invalid)?.known_spells = ticket.after.clone();
        entry.staff_spell = None;
        Ok(())
    }
    pub(crate) fn reject_staff_spell(&mut self, ticket: &StaffSpellTicket) -> Result<(), E> {
        self.validate_staff_spell(ticket)?;
        self.entries
            .get_mut(&ticket.context.actor)
            .ok_or(E::NotBound)?
            .staff_spell = None;
        Ok(())
    }
}
