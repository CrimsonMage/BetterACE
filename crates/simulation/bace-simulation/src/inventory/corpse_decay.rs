//! Extend an unsubmitted valuable proposal with a reserved corpse metadata write.
use super::*;
impl Inventory {
    pub(crate) fn touch_reserved_corpse(
        &mut self,
        operation: u64,
        corpse: EntityId,
    ) -> Result<(), Error> {
        let pending = self.pending.get(&operation).ok_or(Error::InvalidState)?;
        let before = self.items.get(&corpse).ok_or(Error::MissingItem)?;
        if pending.submitted
            || self.reserved.get(&corpse) != Some(&operation)
            || !before.is_container
            || before.place != ItemPlace::World
            || !pending
                .ticket
                .proposal
                .participants
                .contains(&(corpse, before.revision))
        {
            return Err(Error::InvalidState);
        }
        if let Some(change) = pending
            .ticket
            .proposal
            .changes
            .iter()
            .find(|c| c.after.id == corpse)
        {
            return if change.before.as_ref() == Some(before)
                && change.after.place == ItemPlace::World
                && change.after.revision == before.revision.checked_add(1).ok_or(Error::Overflow)?
            {
                Ok(())
            } else {
                Err(Error::InvalidState)
            };
        }
        if pending.ticket.proposal.changes.len() >= 1024 {
            return Err(Error::Capacity);
        }
        let mut after = before.clone();
        after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
        let change = bace_inventory::ItemChange {
            before: Some(before.clone()),
            after,
        };
        self.pending
            .get_mut(&operation)
            .expect("validated pending corpse")
            .ticket
            .proposal
            .changes
            .push(change);
        Ok(())
    }
}
