//! Pinned ACE Player_Inventory action validation and atomic proposals. Geometry,
//! use-chain completion and access generations come only from the world owner.
use crate::model::*;
use bace_gameplay_api::{InventoryRejection as Error, InventoryRequest};
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
impl InventoryView<'_> {
    /// Reuse the proposal owner's bounded container ancestry walk when a
    /// valuable operation freezes intermediate private burden updates.
    pub fn actor_burden(&self, actor: EntityId) -> Result<u64, Error> {
        if actor.0 == 0 || self.items.len() > 4096 || self.containers.len() > 1024 {
            return Err(Error::Capacity);
        }
        self.container(actor)?;
        self.items.iter().try_fold(0u64, |total, item| {
            if self.owner(item)? == Some(actor) {
                total
                    .checked_add(u64::from(item.stack) * u64::from(item.unit_burden))
                    .ok_or(Error::Overflow)
            } else {
                Ok(total)
            }
        })
    }
    pub(crate) fn item(&self, id: EntityId) -> Result<&InventoryItem, Error> {
        self.items
            .iter()
            .find(|i| i.id == id)
            .ok_or(Error::MissingItem)
    }
    pub(crate) fn container(&self, id: EntityId) -> Result<&InventoryContainer, Error> {
        self.containers
            .iter()
            .find(|i| i.id == id)
            .ok_or(Error::MissingContainer)
    }
    pub(crate) fn ancestry(&self, id: EntityId) -> Result<Vec<EntityId>, Error> {
        let mut result = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            if result.len() >= 64 || result.contains(&id) {
                return Err(Error::InvalidState);
            }
            result.push(id);
            next = match self.items.iter().find(|i| i.id == id).map(|i| i.place) {
                Some(ItemPlace::Contained { container, .. }) => Some(container),
                _ => None,
            };
        }
        Ok(result)
    }
    pub(crate) fn owner(&self, item: &InventoryItem) -> Result<Option<EntityId>, Error> {
        match item.place {
            ItemPlace::Contained { container, .. } => self.container_owner(container),
            _ => Ok(None),
        }
    }
    pub(crate) fn container_owner(&self, container: EntityId) -> Result<Option<EntityId>, Error> {
        let chain = self.ancestry(container)?;
        let root = *chain.last().ok_or(Error::InvalidState)?;
        if let Some(item) = self.items.iter().find(|i| i.id == root) {
            return match item.place {
                ItemPlace::World | ItemPlace::Removed => Ok(None),
                _ => Err(Error::InvalidState),
            };
        }
        Ok(self.container(root)?.root_owner)
    }
    fn check_access(
        &self,
        container: EntityId,
        actor: EntityId,
        generation: Option<u64>,
    ) -> Result<(), Error> {
        let c = self.container(container)?;
        if !c.accessible {
            return Err(Error::AccessDenied);
        }
        if self.container_owner(container)? != Some(actor) {
            if !c.open {
                return Err(Error::AccessDenied);
            }
            if generation != Some(c.generation) {
                return Err(Error::StaleView);
            }
        }
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.items.len() > 4096 || self.containers.len() > 1024 {
            return Err(Error::Capacity);
        }
        let mut ids = BTreeSet::new();
        for i in self.items {
            if i.id.0 == 0
                || i.id.0 == u32::MAX
                || !ids.insert(i.id)
                || i.stack == 0 && i.place != ItemPlace::Removed
                || i.stack > i.maximum_stack
                || i.maximum_stack == 0
            {
                return Err(Error::InvalidState);
            }
        }
        let mut ids = BTreeSet::new();
        for c in self.containers {
            if c.id.0 == 0 || !ids.insert(c.id) {
                return Err(Error::InvalidState);
            }
        }
        Ok(())
    }
}
pub fn propose_inventory(
    request: InventoryRequest,
    authority: InventoryAuthority,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    propose_inventory_inner(request, authority, view, None)
}

pub(crate) fn propose_inventory_inner(
    request: InventoryRequest,
    authority: InventoryAuthority,
    view: InventoryView<'_>,
    fresh: Option<InventoryItem>,
) -> Result<InventoryProposal, Error> {
    propose_inventory_policy(request, authority, view, fresh, false)
}
/// Called only after source slot policy has checked the complete revision-fenced
/// equipment roster. It does not auto-dequip overlapping items.
pub fn propose_equipment_inventory(
    request: InventoryRequest,
    authority: InventoryAuthority,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    if !matches!(
        request,
        InventoryRequest::Equip { .. } | InventoryRequest::Move { .. }
    ) {
        return Err(Error::InvalidEquip);
    }
    propose_inventory_policy(request, authority, view, None, true)
}
fn propose_inventory_policy(
    request: InventoryRequest,
    authority: InventoryAuthority,
    view: InventoryView<'_>,
    fresh: Option<InventoryItem>,
    checked_slots: bool,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    if authority.busy {
        return Err(Error::Busy);
    }
    if !authority.geometry_ready {
        return Err(Error::MissingGeometry);
    }
    if !authority.in_range {
        return Err(Error::OutOfRange);
    }
    if !authority.clear_path {
        return Err(Error::Obstructed);
    }
    let (id, destination, split, merge) = match request {
        InventoryRequest::Move {
            item,
            container,
            placement,
        } => (
            item,
            Some(ItemPlace::Contained {
                container,
                slot: placement.try_into().map_err(|_| Error::InvalidPlacement)?,
                equipped: 0,
            }),
            None,
            None,
        ),
        InventoryRequest::Drop { item } => (item, Some(ItemPlace::World), None, None),
        InventoryRequest::Equip { item, location } => (
            item,
            Some(ItemPlace::Contained {
                container: authority.actor,
                slot: location,
                equipped: location,
            }),
            None,
            None,
        ),
        InventoryRequest::SplitToContainer {
            item,
            container,
            placement,
            amount,
        } => (
            item,
            Some(ItemPlace::Contained {
                container,
                slot: placement.try_into().map_err(|_| Error::InvalidPlacement)?,
                equipped: 0,
            }),
            Some(amount),
            None,
        ),
        InventoryRequest::SplitToWorld { item, amount } => {
            (item, Some(ItemPlace::World), Some(amount), None)
        }
        InventoryRequest::SplitToWield {
            item,
            location,
            amount,
        } => (
            item,
            Some(ItemPlace::Contained {
                container: authority.actor,
                slot: location,
                equipped: location,
            }),
            Some(amount),
            None,
        ),
        InventoryRequest::Merge {
            source,
            target,
            amount,
        } => (source, None, Some(amount), Some(target)),
    };
    let original = view.item(id)?.clone();
    if original.place == ItemPlace::Removed {
        return Err(Error::MissingItem);
    }
    if let ItemPlace::Contained { container, .. } = original.place {
        view.check_access(container, authority.actor, authority.source_view)?;
    }
    if view
        .items
        .iter()
        .any(|child| child.trade_reserved && view.ancestry(child.id).is_ok_and(|p| p.contains(&id)))
    {
        return Err(Error::TradeReserved);
    }
    if !original.quest_allowed {
        return Err(Error::Quest);
    }
    let mut changes = Vec::new();
    let mut moved = original.clone();
    let source_owner = view.owner(&original)?;
    let mut motion = original.place == ItemPlace::World;
    if let Some(target_id) = merge {
        if target_id == id || original.is_container {
            return Err(Error::InvalidCount);
        }
        let target = view.item(target_id)?.clone();
        if target.place == ItemPlace::Removed
            || target.template != original.template
            || target.stack_key != original.stack_key
            || target.maximum_stack != original.maximum_stack
            || target.trade_reserved
        {
            return Err(Error::InvalidState);
        }
        let ItemPlace::Contained { container, .. } = target.place else {
            return Err(Error::InvalidPlacement);
        };
        view.check_access(container, authority.actor, authority.destination_view)?;
        let amount =
            u32::try_from(split.ok_or(Error::InvalidCount)?).map_err(|_| Error::InvalidCount)?;
        if amount == 0 || amount > original.stack {
            return Err(Error::InvalidCount);
        }
        let total = target
            .stack
            .checked_add(amount)
            .filter(|n| *n <= target.maximum_stack)
            .ok_or(Error::InvalidCount)?;
        if original.attuned
            && source_owner == Some(authority.actor)
            && view.owner(&target)? != Some(authority.actor)
        {
            return Err(Error::Attuned);
        }
        let mut after_target = target.clone();
        after_target.stack = total;
        after_target.revision = target.revision.checked_add(1).ok_or(Error::Overflow)?;
        changes.push(ItemChange {
            before: Some(target),
            after: after_target,
        });
        moved.stack -= amount;
        if moved.stack == 0 {
            moved.place = ItemPlace::Removed;
        }
    } else {
        let destination = destination.ok_or(Error::InvalidPlacement)?;
        if let Some(amount) = split {
            let amount = u32::try_from(amount).map_err(|_| Error::InvalidCount)?;
            if amount == 0 || amount >= original.stack || original.is_container {
                return Err(Error::InvalidCount);
            }
            let new = authority.new_item.ok_or(Error::Capacity)?;
            if !(0x80000000..=0xfffffffe).contains(&new.0) || view.items.iter().any(|i| i.id == new)
            {
                return Err(Error::InvalidState);
            }
            let mut remainder = original.clone();
            remainder.stack -= amount;
            remainder.revision = remainder.revision.checked_add(1).ok_or(Error::Overflow)?;
            changes.push(ItemChange {
                before: Some(original.clone()),
                after: remainder,
            });
            moved = fresh.ok_or(Error::InvalidState)?;
            if moved.id != new
                || moved.template != original.template
                || moved.revision != 0
                || moved.maximum_stack < amount
                || moved.is_container
                || moved.trade_reserved
                || moved.active_pet
            {
                return Err(Error::InvalidState);
            }
            moved.stack = amount;
        }
        if let ItemPlace::Contained {
            container,
            equipped,
            ..
        } = destination
        {
            view.check_access(container, authority.actor, authority.destination_view)?;
            if view.ancestry(container)?.contains(&original.id) {
                return Err(Error::InvalidState);
            }
            let destination_owner = view.container_owner(container)?;
            if source_owner == Some(authority.actor)
                && destination_owner != Some(authority.actor)
                && view.items.iter().any(|child| {
                    child.active_pet
                        && view
                            .ancestry(child.id)
                            .is_ok_and(|p| p.contains(&original.id))
                })
            {
                return Err(Error::ActivePet);
            }
            if original.attuned
                && source_owner == Some(authority.actor)
                && destination_owner != Some(authority.actor)
            {
                return Err(Error::Attuned);
            }
            for child in view.items {
                if child.attuned
                    && view.ancestry(child.id)?.contains(&original.id)
                    && source_owner == Some(authority.actor)
                    && destination_owner != Some(authority.actor)
                {
                    return Err(Error::Attuned);
                }
            }
            if moved.unique
                && view.items.iter().any(|i| {
                    i.id != original.id
                        && i.template == moved.template
                        && view.owner(i).ok() == Some(destination_owner)
                })
            {
                return Err(Error::UniqueItem);
            }
            if equipped != 0 {
                if container != authority.actor
                    || moved.is_container
                    || !moved.wield_requirements_met
                {
                    return Err(Error::Requirements);
                }
                if !checked_slots && equipped & moved.valid_wield != equipped {
                    return Err(Error::InvalidEquip);
                }
                for other in view.items {
                    if checked_slots || other.id == id {
                        continue;
                    }
                    if let ItemPlace::Contained {
                        container: owner,
                        equipped: mask,
                        ..
                    } = other.place
                        && owner == authority.actor
                        && (mask & equipped != 0
                            || mask & moved.incompatible_wield != 0
                            || other.incompatible_wield & equipped != 0)
                    {
                        if other.trade_reserved || other.active_pet {
                            return Err(Error::Busy);
                        }
                        let mut unequipped = other.clone();
                        unequipped.place = ItemPlace::Contained {
                            container: authority.actor,
                            slot: 0,
                            equipped: 0,
                        };
                        unequipped.revision =
                            unequipped.revision.checked_add(1).ok_or(Error::Overflow)?;
                        changes.push(ItemChange {
                            before: Some(other.clone()),
                            after: unequipped,
                        });
                    }
                }
            }
        } else if destination == ItemPlace::World {
            if source_owner != Some(authority.actor) {
                return Err(Error::OwnershipMismatch);
            }
            if !authority.drop_validated {
                return Err(Error::MissingGeometry);
            }
            if view.items.iter().any(|child| {
                child.active_pet && view.ancestry(child.id).is_ok_and(|p| p.contains(&id))
            }) {
                return Err(Error::ActivePet);
            }
            if view
                .items
                .iter()
                .any(|i| i.attuned && view.ancestry(i.id).is_ok_and(|p| p.contains(&id)))
            {
                return Err(Error::Attuned);
            }
            motion = true;
        }
        moved.place = destination;
    }
    moved.revision = moved.revision.checked_add(1).ok_or(Error::Overflow)?;
    let before = if moved.id == original.id {
        Some(original.clone())
    } else {
        None
    };
    changes.push(ItemChange {
        before,
        after: moved,
    });
    finish_proposal(&view, authority.actor, changes, motion)
}
pub(crate) fn finish_proposal(
    view: &InventoryView<'_>,
    actor: EntityId,
    changes: Vec<ItemChange>,
    motion: bool,
) -> Result<InventoryProposal, Error> {
    let mut resulting: BTreeMap<EntityId, InventoryItem> =
        view.items.iter().cloned().map(|i| (i.id, i)).collect();
    // ACE removes close gaps, then insertion shifts the selected slot and all
    // later slots in the same main/backpack lane. Every shifted item participates.
    for change in &changes {
        if let Some(before) = &change.before
            && before.place != change.after.place
        {
            let removed = resulting.remove(&before.id).ok_or(Error::InvalidState)?;
            if let ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            } = removed.place
            {
                for item in resulting.values_mut() {
                    if let ItemPlace::Contained {
                        container: id,
                        slot: at,
                        equipped: 0,
                    } = item.place
                        && id == container
                        && item.pack_slot == before.pack_slot
                        && at > slot
                    {
                        item.place = ItemPlace::Contained {
                            container: id,
                            slot: at - 1,
                            equipped: 0,
                        };
                    }
                }
            }
        }
    }
    for change in &changes {
        if change
            .before
            .as_ref()
            .is_none_or(|b| b.place != change.after.place)
            && let ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            } = change.after.place
        {
            for item in resulting.values_mut() {
                if let ItemPlace::Contained {
                    container: id,
                    slot: at,
                    equipped: 0,
                } = item.place
                    && id == container
                    && item.pack_slot == change.after.pack_slot
                    && at >= slot
                {
                    item.place = ItemPlace::Contained {
                        container: id,
                        slot: at.checked_add(1).ok_or(Error::Overflow)?,
                        equipped: 0,
                    };
                }
            }
        }
        let mut updated = change.after.clone();
        if change
            .before
            .as_ref()
            .is_some_and(|b| b.place == change.after.place)
            && let Some(shifted) = resulting.get(&updated.id)
        {
            updated.place = shifted.place;
        }
        resulting.insert(updated.id, updated);
    }
    let mut changes = Vec::new();
    for item in resulting.values_mut() {
        let old = view.items.iter().find(|old| old.id == item.id);
        if old.is_none_or(|old| old != item) {
            if let Some(old) = old {
                item.revision = old.revision.checked_add(1).ok_or(Error::Overflow)?;
            }
            changes.push(ItemChange {
                before: old.cloned(),
                after: item.clone(),
            });
        }
    }
    let resulting: Vec<_> = resulting
        .into_values()
        .filter(|i| i.place != ItemPlace::Removed)
        .collect();
    let after = InventoryView {
        items: &resulting,
        containers: view.containers,
    };
    let actor_burden = after.actor_burden(actor)?;
    if actor_burden > after.container(actor)?.burden_limit {
        return Err(Error::Burden);
    }
    for container in view.containers {
        let mut slots = BTreeSet::new();
        let mut normal = 0;
        let mut packs = 0;
        for item in &resulting {
            if let ItemPlace::Contained {
                container: id,
                slot,
                equipped,
            } = item.place
                && id == container.id
                && equipped == 0
            {
                if !slots.insert((item.pack_slot, slot)) {
                    return Err(Error::InvalidPlacement);
                }
                if item.pack_slot {
                    packs += 1;
                    if slot >= container.pack_slots {
                        return Err(Error::Capacity);
                    }
                } else {
                    normal += 1;
                    if slot >= container.slots {
                        return Err(Error::Capacity);
                    }
                }
            }
        }
        if normal > container.slots || packs > container.pack_slots {
            return Err(Error::Capacity);
        }
    }
    let mut participants = BTreeMap::new();
    for change in &changes {
        for item in change.before.iter().chain(std::iter::once(&change.after)) {
            for id in view.ancestry(item.id).unwrap_or_else(|_| vec![item.id]) {
                let revision = view
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .map(|i| i.revision)
                    .or_else(|| {
                        view.containers
                            .iter()
                            .find(|c| c.id == id)
                            .map(|c| c.revision)
                    })
                    .unwrap_or(0);
                participants.insert(id, revision);
            }
            if let ItemPlace::Contained { container, .. } = item.place {
                for id in view.ancestry(container)? {
                    let revision = view
                        .items
                        .iter()
                        .find(|i| i.id == id)
                        .map(|i| i.revision)
                        .or_else(|| {
                            view.containers
                                .iter()
                                .find(|c| c.id == id)
                                .map(|c| c.revision)
                        })
                        .ok_or(Error::MissingContainer)?;
                    participants.insert(id, revision);
                }
            }
        }
    }
    Ok(InventoryProposal {
        changes,
        participants: participants.into_iter().collect(),
        actor_burden,
        requires_pickup_motion: motion,
    })
}
/// Server-only reward removal/grant uses the same graph checks. Selection, name
/// approval and script eligibility belong to the emote/quest owner beforehand.
pub fn propose_take(
    actor: EntityId,
    item: EntityId,
    count: u32,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    let before = view.item(item)?.clone();
    if view.owner(&before)? != Some(actor) {
        return Err(Error::OwnershipMismatch);
    }
    if before.trade_reserved || before.active_pet {
        return Err(Error::Busy);
    }
    if count == 0 || count > before.stack || before.is_container && count != before.stack {
        return Err(Error::InvalidCount);
    }
    if before.is_container
        && view
            .items
            .iter()
            .any(|i| i.id != item && view.ancestry(i.id).is_ok_and(|a| a.contains(&item)))
    {
        return Err(Error::InvalidState);
    }
    let mut after = before.clone();
    after.stack -= count;
    if after.stack == 0 {
        after.place = ItemPlace::Removed
    }
    after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
    finish_proposal(
        &view,
        actor,
        vec![ItemChange {
            before: Some(before),
            after,
        }],
        false,
    )
}
pub fn propose_grant(
    actor: EntityId,
    item: InventoryItem,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    if view.items.iter().any(|i| i.id == item.id)
        || !(0x80000000..=0xfffffffe).contains(&item.id.0)
        || item.stack == 0
        || item.stack > item.maximum_stack
    {
        return Err(Error::InvalidState);
    }
    let ItemPlace::Contained { container, .. } = item.place else {
        return Err(Error::InvalidPlacement);
    };
    if view.container_owner(container)? != Some(actor) {
        return Err(Error::AccessDenied);
    }
    if item.unique
        && view
            .items
            .iter()
            .any(|i| i.template == item.template && view.owner(i).ok() == Some(Some(actor)))
    {
        return Err(Error::UniqueItem);
    }
    finish_proposal(
        &view,
        actor,
        vec![ItemChange {
            before: None,
            after: item,
        }],
        false,
    )
}

/// Native TakeItems: use inventory stacks first; only fall back to equipped
/// items when no matching inventory item exists. Count caps at available units,
/// matching pinned TryConsumeFromInventoryWithNetworking rather than fabricating
/// the missing units. One proposal contains every removal and slot shift.
pub fn propose_take_template(
    actor: EntityId,
    template: u32,
    count: Option<u32>,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    if template == 0 || count == Some(0) {
        return Err(Error::InvalidCount);
    }
    let mut selected: Vec<_> = view
        .items
        .iter()
        .filter(|i| i.template == template && view.owner(i).ok() == Some(Some(actor)))
        .collect();
    let inventory = selected
        .iter()
        .any(|i| matches!(i.place, ItemPlace::Contained { equipped: 0, .. }));
    selected.retain(
        |i| matches!(i.place,ItemPlace::Contained{equipped,..} if (equipped==0)==inventory),
    );
    selected.sort_by_key(|i| inventory_order(&view, i));
    if selected.is_empty() {
        return Err(Error::MissingItem);
    }
    let mut left = u64::from(count.unwrap_or(i32::MAX as u32));
    let mut changes = Vec::new();
    for item in selected {
        if left == 0 {
            break;
        }
        if item.trade_reserved || item.active_pet {
            return Err(Error::Busy);
        }
        if item.is_container
            && view.items.iter().any(|child| {
                child.id != item.id && view.ancestry(child.id).is_ok_and(|a| a.contains(&item.id))
            })
        {
            return Err(Error::InvalidState);
        }
        let amount = left.min(u64::from(item.stack)) as u32;
        left -= u64::from(amount);
        let mut after = item.clone();
        after.stack -= amount;
        if after.stack == 0 {
            after.place = ItemPlace::Removed
        }
        after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
        changes.push(ItemChange {
            before: Some(item.clone()),
            after,
        });
    }
    finish_proposal(&view, actor, changes, false)
}
pub(crate) fn inventory_order(view: &InventoryView<'_>, item: &InventoryItem) -> Vec<(bool, u32)> {
    let mut path = Vec::new();
    let mut current = item;
    for _ in 0..64 {
        let ItemPlace::Contained {
            container, slot, ..
        } = current.place
        else {
            break;
        };
        path.push((current.pack_slot, slot));
        let Some(parent) = view.items.iter().find(|i| i.id == container) else {
            break;
        };
        current = parent;
    }
    path.reverse();
    path
}

/// Validate immutable changes prepared by an owning gameplay subsystem against
/// the same inventory graph, ancestor revisions, capacity and burden invariants.
/// This is not authorization: caller checks recipe/device policy and ownership.
pub fn propose_item_changes(
    actor: EntityId,
    changes: Vec<ItemChange>,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    if changes.is_empty() || changes.len() > 1024 {
        return Err(Error::Capacity);
    }
    let mut ids = std::collections::BTreeSet::new();
    for change in &changes {
        if !ids.insert(change.after.id) {
            return Err(Error::InvalidState);
        }
        if let Some(before) = &change.before {
            if view.items.iter().find(|item| item.id == before.id) != Some(before)
                || before.id != change.after.id
            {
                return Err(Error::StaleView);
            }
        } else if view.items.iter().any(|item| item.id == change.after.id) {
            return Err(Error::InvalidState);
        }
    }
    finish_proposal(&view, actor, changes, false)
}
