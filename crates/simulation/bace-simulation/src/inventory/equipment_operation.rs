//! Equipment proposals fence the complete gear roster while retaining one owner.
use super::*;
impl Inventory {
    pub(crate) fn apply_equipment(
        &mut self,
        request: InventoryRequest,
        authority: InventoryAuthority,
        split: Option<bace_inventory::StackSplitPreparation>,
    ) -> Result<u64, Error> {
        let target = match request {
            InventoryRequest::Equip { item, .. } | InventoryRequest::Move { item, .. } => item,
            InventoryRequest::SplitToWield { item, .. } => item,
            _ => return Err(Error::InvalidEquip),
        };
        if matches!(request, InventoryRequest::SplitToWield { .. }) != split.is_some() {
            return Err(Error::InvalidState);
        }
        let mut items = Vec::new();
        for item in self
            .items
            .values()
            .filter(|i| self.owned(authority.actor, i.id))
        {
            if items.len() == 1023 {
                return Err(Error::Capacity);
            }
            items.push(item.clone());
        }
        if split.is_none() {
            let item = items
                .iter_mut()
                .find(|i| i.id == target)
                .ok_or(Error::OwnershipMismatch)?;
            // The Kernel has just evaluated the actual accepted requirements.
            item.wield_requirements_met = true;
        }
        let mut containers = vec![
            *self
                .container(authority.actor)
                .ok_or(Error::MissingContainer)?,
        ];
        for item in &items {
            if item.is_container {
                containers.push(*self.container(item.id).ok_or(Error::MissingContainer)?);
            }
        }
        let view = InventoryView {
            items: &items,
            containers: &containers,
        };
        let mut proposal = if let Some(split) = split {
            bace_inventory::propose_stack_split(request, authority, split, view)?
        } else {
            bace_inventory::propose_equipment_inventory(request, authority, view)?
        };
        for change in &mut proposal.changes {
            if let Some(before) = &mut change.before {
                *before = self.item(before.id).ok_or(Error::MissingItem)?.clone();
            }
        }
        for item in self.equipped_items(authority.actor) {
            if !proposal.participants.iter().any(|(id, _)| *id == item.id) {
                proposal.participants.push((item.id, item.revision));
            }
        }
        if proposal.participants.len() > 1024 {
            return Err(Error::Capacity);
        }
        proposal.participants.sort_unstable();
        self.reserve(authority.actor, proposal)
    }
}

#[cfg(test)]
mod tests;
