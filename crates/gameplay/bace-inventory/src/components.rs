//! ACE spell component WCID validation/burning. Formula component IDs have already
//! been resolved through the verified DualDidMapper by the magic owner. Burn draws
//! are supplied once and never rerolled here or on persistence retry.
use crate::{InventoryProposal, InventoryView, ItemChange, ItemPlace};
use bace_gameplay_api::InventoryRejection as Error;
use bace_types::EntityId;
use std::collections::BTreeMap;
fn quantities(values: &[(u32, u32)]) -> Result<BTreeMap<u32, u32>, Error> {
    if values.len() > 64 {
        return Err(Error::Capacity);
    }
    let mut totals = BTreeMap::new();
    for &(id, count) in values {
        if id == 0 || count == 0 {
            return Err(Error::InvalidCount);
        }
        let n = totals.entry(id).or_insert(0u32);
        *n = n.checked_add(count).ok_or(Error::Overflow)?;
    }
    Ok(totals)
}
pub fn has_required_components(
    actor: EntityId,
    required: &[(u32, u32)],
    view: InventoryView<'_>,
) -> Result<bool, Error> {
    view.validate()?;
    let required = quantities(required)?;
    for (id, count) in required {
        let available: u64 = view
            .items
            .iter()
            .filter(|i| {
                i.template == id
                    && !i.trade_reserved
                    && !i.active_pet
                    && matches!(i.place, ItemPlace::Contained { equipped: 0, .. })
                    && view.owner(i).ok() == Some(Some(actor))
            })
            .map(|i| u64::from(i.stack))
            .sum();
        if available < u64::from(count) {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn propose_component_use(
    actor: EntityId,
    required: &[(u32, u32)],
    consumed: &[(u32, u32)],
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    if !has_required_components(
        actor,
        required,
        InventoryView {
            items: view.items,
            containers: view.containers,
        },
    )? {
        return Err(Error::Requirements);
    }
    let required = quantities(required)?;
    let consumed = quantities(consumed)?;
    if consumed
        .iter()
        .any(|(id, count)| required.get(id).is_none_or(|n| count > n))
    {
        return Err(Error::InvalidCount);
    }
    let mut changes = Vec::new();
    let mut participants = BTreeMap::new();
    for (template, needed) in required {
        let mut items: Vec<_> = view
            .items
            .iter()
            .filter(|i| {
                i.template == template
                    && matches!(i.place, ItemPlace::Contained { equipped: 0, .. })
                    && view.owner(i).ok() == Some(Some(actor))
            })
            .collect();
        items.sort_by_key(|i| crate::actions::inventory_order(&view, i));
        let mut reserve = needed;
        let mut burn = consumed.get(&template).copied().unwrap_or(0);
        for item in items {
            if reserve == 0 && burn == 0 {
                break;
            }
            if item.trade_reserved || item.active_pet || item.is_container {
                return Err(Error::Busy);
            }
            let take = burn.min(item.stack);
            burn -= take;
            reserve = reserve.saturating_sub(item.stack);
            for id in view.ancestry(item.id)? {
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
                    .ok_or(Error::InvalidState)?;
                participants.insert(id, revision);
            }
            if take > 0 {
                let mut after = item.clone();
                after.stack -= take;
                if after.stack == 0 {
                    after.place = ItemPlace::Removed
                }
                after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
                changes.push(ItemChange {
                    before: Some(item.clone()),
                    after,
                });
            }
        }
        if reserve != 0 || burn != 0 {
            return Err(Error::Requirements);
        }
    }
    let mut proposal = crate::actions::finish_proposal(&view, actor, changes, false)?;
    for (id, revision) in proposal.participants {
        participants.insert(id, revision);
    }
    proposal.participants = participants.into_iter().collect();
    Ok(proposal)
}
/// Exact-ID payment consumption. Every requested stack must be present and large
/// enough; unlike native emote TakeItems, a shortage never becomes partial success.
pub fn propose_take_items(
    actor: EntityId,
    requested: &[(EntityId, u32)],
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    if requested.is_empty() || requested.len() > 1024 {
        return Err(Error::InvalidCount);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut changes = Vec::with_capacity(requested.len());
    for &(id, count) in requested {
        if count == 0 || !ids.insert(id) {
            return Err(Error::InvalidCount);
        }
        let before = view.item(id)?.clone();
        if view.owner(&before)? != Some(actor) {
            return Err(Error::OwnershipMismatch);
        }
        if count > before.stack || before.trade_reserved || before.active_pet {
            return Err(Error::InvalidCount);
        }
        if before.is_container
            && view.items.iter().any(|child| {
                child.id != id && view.ancestry(child.id).is_ok_and(|a| a.contains(&id))
            })
        {
            return Err(Error::InvalidState);
        }
        let mut after = before.clone();
        after.stack -= count;
        if after.stack == 0 {
            after.place = ItemPlace::Removed
        }
        after.revision = after.revision.checked_add(1).ok_or(Error::Overflow)?;
        changes.push(ItemChange {
            before: Some(before),
            after,
        });
    }
    crate::actions::finish_proposal(&view, actor, changes, false)
}
