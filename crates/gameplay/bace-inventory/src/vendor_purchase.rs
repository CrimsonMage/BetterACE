//! One accepted inventory graph for a vendor currency debit and fresh grant.
//! The commerce owner supplies the source-priced cost, selected currency stack
//! identities, and complete factory items before this pure proposal is called.
use crate::{InventoryItem, InventoryProposal, InventoryView, ItemChange, ItemPlace};
use bace_gameplay_api::InventoryRejection as Error;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};

/// Pinned Container.GetInventoryItemsOfWCID traversal: direct stacks in
/// placement order, then each direct side container in placement order.
/// This runs over the accepted immutable graph, not client-selected IDs.
pub fn select_vendor_currency_debits(
    actor: EntityId,
    currency_template: u32,
    exact_cost: u32,
    view: InventoryView<'_>,
) -> Result<Vec<(EntityId, u32)>, Error> {
    view.validate()?;
    if currency_template == 0 || exact_cost == 0 {
        return Err(Error::InvalidCount);
    }
    let root = view.container(actor)?;
    if root.root_owner != Some(actor) || !root.accessible {
        return Err(Error::AccessDenied);
    }
    let mut children: BTreeMap<EntityId, Vec<&InventoryItem>> = BTreeMap::new();
    for item in view.items {
        if let ItemPlace::Contained {
            container,
            equipped: 0,
            ..
        } = item.place
        {
            children.entry(container).or_default().push(item);
        }
    }
    for direct in children.values_mut() {
        direct.sort_by_key(|item| match item.place {
            ItemPlace::Contained { slot, .. } => (slot, item.id),
            _ => (u32::MAX, item.id),
        });
    }
    let mut walk = CurrencyWalk {
        actor,
        currency_template,
        view: &view,
        children,
        visited: BTreeSet::new(),
        left: exact_cost,
        debits: Vec::new(),
    };
    walk.visit(actor, 0)?;
    if walk.left != 0 {
        return Err(Error::Requirements);
    }
    Ok(walk.debits)
}

struct CurrencyWalk<'view, 'items> {
    actor: EntityId,
    currency_template: u32,
    view: &'view InventoryView<'items>,
    children: BTreeMap<EntityId, Vec<&'items InventoryItem>>,
    visited: BTreeSet<EntityId>,
    left: u32,
    debits: Vec<(EntityId, u32)>,
}

impl CurrencyWalk<'_, '_> {
    fn visit(&mut self, container: EntityId, depth: usize) -> Result<(), Error> {
        if depth >= 64 || self.visited.len() >= 1024 || !self.visited.insert(container) {
            return Err(Error::InvalidState);
        }
        let metadata = self.view.container(container)?;
        if !metadata.accessible || self.view.container_owner(container)? != Some(self.actor) {
            return Err(Error::AccessDenied);
        }
        let direct = self.children.get(&container).cloned().unwrap_or_default();
        for item in &direct {
            if self.left == 0 {
                break;
            }
            if item.template != self.currency_template {
                continue;
            }
            if item.trade_reserved || item.active_pet || item.is_container {
                return Err(Error::Busy);
            }
            let count = self.left.min(item.stack);
            self.left -= count;
            self.debits.push((item.id, count));
        }
        if self.left != 0 {
            for item in direct {
                if self.left == 0 {
                    break;
                }
                if item.is_container {
                    self.visit(item.id, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}

/// Reserve exact, server-selected currency and fresh leaf items in one proposal.
/// The caller must derive `debits` from accepted inventory, never client IDs.
/// Container-valued purchases need a separate prepared-forest contract.
pub fn propose_vendor_purchase(
    actor: EntityId,
    currency_template: u32,
    exact_cost: u32,
    debits: &[(EntityId, u32)],
    prepared: &[InventoryItem],
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    if currency_template == 0
        || exact_cost == 0
        || debits.is_empty()
        || prepared.is_empty()
        || debits
            .len()
            .checked_add(prepared.len())
            .is_none_or(|n| n > 1024)
    {
        return Err(Error::InvalidCount);
    }
    if view
        .items
        .len()
        .checked_add(prepared.len())
        .is_none_or(|n| n > 4096)
    {
        return Err(Error::Capacity);
    }
    let root = view.container(actor)?;
    if root.root_owner != Some(actor) || !root.accessible {
        return Err(Error::AccessDenied);
    }

    let mut ids: BTreeSet<_> = view.items.iter().map(|item| item.id).collect();
    ids.extend(view.containers.iter().map(|container| container.id));
    let mut selected = BTreeSet::new();
    let mut changes = Vec::with_capacity(debits.len() + prepared.len());
    let mut removed = BTreeSet::new();
    let mut paid = 0_u32;
    for &(id, count) in debits {
        if count == 0 || !selected.insert(id) {
            return Err(Error::InvalidCount);
        }
        let before = view.item(id)?;
        if before.template != currency_template
            || !matches!(before.place, ItemPlace::Contained { equipped: 0, .. })
            || view.owner(before)? != Some(actor)
        {
            return Err(Error::OwnershipMismatch);
        }
        if before.trade_reserved || before.active_pet || before.is_container {
            return Err(Error::Busy);
        }
        if count > before.stack {
            return Err(Error::Requirements);
        }
        paid = paid.checked_add(count).ok_or(Error::Overflow)?;
        let mut after = before.clone();
        after.stack -= count;
        if after.stack == 0 {
            after.place = ItemPlace::Removed;
            removed.insert(id);
        }
        after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
        changes.push(ItemChange {
            before: Some(before.clone()),
            after,
        });
    }
    if paid != exact_cost {
        return Err(Error::Requirements);
    }

    // ACE inserts each purchased object at main slot zero, then direct side
    // packs when main is full. Count available space after the exact debit.
    let mut occupied: BTreeMap<(EntityId, bool), usize> = BTreeMap::new();
    for item in view.items.iter().filter(|item| !removed.contains(&item.id)) {
        if let ItemPlace::Contained {
            container,
            equipped: 0,
            ..
        } = item.place
        {
            *occupied.entry((container, item.pack_slot)).or_default() += 1;
        }
    }
    let mut side_packs: Vec<_> = view
        .items
        .iter()
        .filter_map(|item| match item.place {
            ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            } if container == actor
                && item.is_container
                && view
                    .containers
                    .iter()
                    .any(|candidate| candidate.id == item.id) =>
            {
                Some((slot, item.id))
            }
            _ => None,
        })
        .collect();
    side_packs.sort_unstable();
    for source in prepared {
        if !(0x8000_0000..=0xffff_fffe).contains(&source.id.0)
            || source.template == 0
            || !ids.insert(source.id)
            || source.revision != 1
            || source.stack == 0
            || source.stack > source.maximum_stack
            || source.is_container
            || source.trade_reserved
            || source.active_pet
            || !matches!(source.place, ItemPlace::Contained { container, equipped: 0, .. } if container == actor)
        {
            return Err(Error::InvalidState);
        }
        if source.unique
            && (view.items.iter().any(|old| {
                !removed.contains(&old.id)
                    && old.template == source.template
                    && view.owner(old).ok() == Some(Some(actor))
            }) || prepared
                .iter()
                .filter(|item| item.template == source.template)
                .count()
                > 1)
        {
            return Err(Error::UniqueItem);
        }
        let room = |container: EntityId,
                    occupied: &BTreeMap<(EntityId, bool), usize>|
         -> Result<bool, Error> {
            let target = view.container(container)?;
            Ok(target.accessible
                && occupied
                    .get(&(container, source.pack_slot))
                    .copied()
                    .unwrap_or(0)
                    < if source.pack_slot {
                        target.pack_slots
                    } else {
                        target.slots
                    } as usize)
        };
        let container = if room(actor, &occupied)? {
            actor
        } else if source.pack_slot {
            return Err(Error::Capacity);
        } else {
            let mut selected = None;
            for (_, id) in &side_packs {
                if room(*id, &occupied)? {
                    selected = Some(*id);
                    break;
                }
            }
            selected.ok_or(Error::Capacity)?
        };
        *occupied.entry((container, source.pack_slot)).or_default() += 1;
        let mut fresh = source.clone();
        fresh.place = ItemPlace::Contained {
            container,
            slot: 0,
            equipped: 0,
        };
        changes.push(ItemChange {
            before: None,
            after: fresh,
        });
    }
    crate::actions::finish_proposal(&view, actor, changes, false)
}
