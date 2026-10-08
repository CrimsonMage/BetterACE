//! Source empty-corpse decay joins the final accepted loot transfer.
#[cfg(test)]
mod tests;
use super::*;
impl Kernel {
    pub(super) fn prepare_inventory_corpse_decay(
        &mut self,
        operation: u64,
    ) -> Result<Vec<crate::CorpseDecayChange>, E> {
        let ticket = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::InvalidState)?;
        let mut candidates = std::collections::BTreeSet::new();
        for change in &ticket.proposal.changes {
            if let Some(before) = &change.before
                && let ItemPlace::Contained { container, .. } = before.place
                && self.corpse_expiry.deadlines.contains_key(&container)
                && !matches!(change.after.place, ItemPlace::Contained { container: after, .. } if after == container)
            {
                candidates.insert(container);
            }
        }
        let limit = self.tick.checked_add(15 * 30).ok_or(E::Overflow)?;
        let mut changes = Vec::new();
        for corpse in candidates {
            let deadline = self.corpse_expiry.deadlines[&corpse];
            if deadline.tick <= limit {
                continue;
            }
            let remains = self.inventory.items().any(|item| {
                let after = ticket.proposal.changes.iter().find(|c| c.after.id == item.id)
                    .map_or(item, |c| &c.after);
                matches!(after.place, ItemPlace::Contained { container, .. } if container == corpse)
            }) || ticket.proposal.changes.iter().any(|c| c.before.is_none()
                && matches!(c.after.place, ItemPlace::Contained { container, .. } if container == corpse));
            if !remains {
                changes.push(crate::CorpseDecayChange {
                    corpse,
                    death_operation: deadline.operation,
                    before_expires_tick: deadline.tick,
                    after_expires_tick: limit,
                    prepared_tick: self.tick,
                });
            }
        }
        for change in &changes {
            self.inventory
                .touch_reserved_corpse(operation, change.corpse)?;
        }
        Ok(changes)
    }
    pub(super) fn validate_inventory_corpse_decay(
        &self,
        changes: &[crate::CorpseDecayChange],
    ) -> Result<(), E> {
        if changes.iter().any(|c| {
            self.corpse_expiry
                .deadlines
                .get(&c.corpse)
                .is_none_or(|d| d.operation != c.death_operation || d.tick != c.before_expires_tick)
        }) {
            return Err(E::InvalidState);
        }
        Ok(())
    }
    pub(super) fn adopt_inventory_corpse_decay(&mut self, changes: &[crate::CorpseDecayChange]) {
        for change in changes {
            self.corpse_expiry
                .deadlines
                .get_mut(&change.corpse)
                .expect("preflighted corpse deadline")
                .tick = change.after_expires_tick;
        }
    }
}
