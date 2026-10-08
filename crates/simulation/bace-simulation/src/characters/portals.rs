//! Portal ownership freezes the aggregate revision through an uncertain receipt.
use super::*;
impl Characters {
    pub(crate) fn can_reserve_portal(
        &self,
        operation: u64,
        participants: &[(EntityId, u64, u64)],
    ) -> Result<(), ()> {
        if operation == 0 || participants.is_empty() || participants.len() > 9 {
            return Err(());
        }
        let mut seen = std::collections::BTreeSet::new();
        for &(actor, before, after) in participants {
            if !seen.insert(actor)
                || self.reserved(actor)
                || before.checked_add(1) != Some(after)
                || self.get(actor).is_none_or(|c| c.revision() != before)
            {
                return Err(());
            }
        }
        Ok(())
    }
    pub(crate) fn reserve_portal(&mut self, operation: u64, participants: &[(EntityId, u64, u64)]) {
        // Caller preflights the complete list on this same owner before resource
        // mutation. No command can interleave between preflight and adoption.
        for &(actor, _, _) in participants {
            self.entries
                .get_mut(&actor)
                .expect("prevalidated portal participant")
                .portal = Some(operation);
        }
    }
    pub(crate) fn release_portal(&mut self, operation: u64, participants: &[(EntityId, u64, u64)]) {
        for &(actor, _, _) in participants {
            if let Some(entry) = self.entries.get_mut(&actor)
                && entry.portal == Some(operation)
            {
                entry.portal = None;
            }
        }
    }
}
