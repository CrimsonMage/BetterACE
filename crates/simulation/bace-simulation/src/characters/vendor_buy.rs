//! A Shop Buy holds the player aggregate until its joined durable receipt.
use super::*;

impl Characters {
    pub(crate) fn reserve_vendor_buy(
        &mut self,
        actor: EntityId,
        operation: u64,
    ) -> Result<u64, ()> {
        if operation == 0 || self.reserved(actor) {
            return Err(());
        }
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        let revision = entry.progression.revision();
        if revision == u64::MAX {
            return Err(());
        }
        entry.vendor_buy = Some(operation);
        Ok(revision)
    }

    pub(crate) fn vendor_buy_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries.get(&actor)?.vendor_buy
    }

    pub(crate) fn finish_vendor_buy(
        &mut self,
        actor: EntityId,
        operation: u64,
        before_revision: u64,
        committed: bool,
    ) -> Result<(), ()> {
        let entry = self.entries.get_mut(&actor).ok_or(())?;
        if entry.vendor_buy != Some(operation)
            || entry.progression.revision() != before_revision
            || before_revision == u64::MAX
        {
            return Err(());
        }
        if committed {
            entry.progression.touch_revision().map_err(|_| ())?;
        }
        entry.vendor_buy = None;
        Ok(())
    }
}
