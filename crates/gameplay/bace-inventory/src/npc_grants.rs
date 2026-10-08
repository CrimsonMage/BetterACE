//! Pinned Player.GiveFromEmote / Container.TryAddToInventory placement: each
//! constructed stack enters main slot zero, then direct side packs by placement.
//! The complete bounded batch is validated before the owner reserves anything.
use crate::{InventoryItem, InventoryProposal, InventoryView, ItemChange, ItemPlace};
use bace_gameplay_api::InventoryRejection as E;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
pub fn propose_npc_grants(
    actor: EntityId,
    prepared: &[InventoryItem],
    view: InventoryView<'_>,
) -> Result<InventoryProposal, E> {
    view.validate()?;
    if prepared.is_empty() || prepared.len() > 1024 || view.items.len() + prepared.len() > 4096 {
        return Err(E::Capacity);
    }
    if view.container(actor)?.root_owner != Some(actor) {
        return Err(E::AccessDenied);
    }
    let mut identities: BTreeSet<_> = view.items.iter().map(|i| i.id).collect();
    let mut resulting = view.items.to_vec();
    for item in prepared {
        if !(0x80000000..=0xfffffffe).contains(&item.id.0)
            || !identities.insert(item.id)
            || item.revision != 1
            || item.stack == 0
            || item.stack > item.maximum_stack
            || item.trade_reserved
            || item.active_pet
        {
            return Err(E::InvalidState);
        }
        let current = InventoryView {
            items: &resulting,
            containers: view.containers,
        };
        if item.unique
            && resulting.iter().any(|old| {
                old.template == item.template && current.owner(old).ok() == Some(Some(actor))
            })
        {
            return Err(E::UniqueItem);
        }
        let room = |container: EntityId| -> Result<bool, E> {
            let target = current.container(container)?;
            if !target.accessible {
                return Ok(false);
            }
            let occupied=resulting.iter().filter(|old|old.pack_slot==item.pack_slot&&matches!(old.place,ItemPlace::Contained{container:id,equipped:0,..}if id==container)).count();
            Ok(occupied
                < if item.pack_slot {
                    target.pack_slots
                } else {
                    target.slots
                } as usize)
        };
        let container = if room(actor)? {
            actor
        } else {
            if item.pack_slot {
                return Err(E::Capacity);
            }
            let mut side: Vec<_> = resulting
                .iter()
                .filter_map(|old| match old.place {
                    ItemPlace::Contained {
                        container,
                        slot,
                        equipped: 0,
                    } if container == actor
                        && old.is_container
                        && view.containers.iter().any(|c| c.id == old.id) =>
                    {
                        Some((slot, old.id))
                    }
                    _ => None,
                })
                .collect();
            side.sort_unstable();
            let mut selected = None;
            for (_, id) in side {
                if room(id)? {
                    selected = Some(id);
                    break;
                }
            }
            selected.ok_or(E::Capacity)?
        };
        let mut fresh = item.clone();
        fresh.place = ItemPlace::Contained {
            container,
            slot: 0,
            equipped: 0,
        };
        for old in &mut resulting {
            if let ItemPlace::Contained {
                container: destination,
                slot,
                equipped: 0,
            } = old.place
                && destination == container
                && old.pack_slot == fresh.pack_slot
            {
                old.place = ItemPlace::Contained {
                    container,
                    slot: slot.checked_add(1).ok_or(E::Overflow)?,
                    equipped: 0,
                };
            }
        }
        resulting.push(fresh);
    }
    let mut changes = Vec::new();
    for mut after in resulting {
        let before = view.items.iter().find(|i| i.id == after.id);
        if before == Some(&after) {
            continue;
        }
        if let Some(before) = before {
            after.revision = before.revision.checked_add(1).ok_or(E::Overflow)?;
        }
        changes.push(ItemChange {
            before: before.cloned(),
            after,
        });
    }
    let mut final_items = view.items.to_vec();
    for change in &changes {
        if let Some(old) = final_items.iter_mut().find(|i| i.id == change.after.id) {
            *old = change.after.clone();
        } else {
            final_items.push(change.after.clone());
        }
    }
    let after = InventoryView {
        items: &final_items,
        containers: view.containers,
    };
    after.validate()?;
    let mut slots = BTreeSet::new();
    for item in &final_items {
        if let ItemPlace::Contained {
            container,
            slot,
            equipped: 0,
        } = item.place
        {
            let limit = after.container(container)?;
            if !slots.insert((container, item.pack_slot, slot)) {
                return Err(E::InvalidPlacement);
            }
            if slot
                >= if item.pack_slot {
                    limit.pack_slots
                } else {
                    limit.slots
                }
            {
                return Err(E::Capacity);
            }
        }
    }
    let actor_burden = final_items.iter().try_fold(0u64, |n, i| {
        if after.owner(i)? == Some(actor) {
            n.checked_add(u64::from(i.stack) * u64::from(i.unit_burden))
                .ok_or(E::Overflow)
        } else {
            Ok(n)
        }
    })?;
    if actor_burden > view.container(actor)?.burden_limit {
        return Err(E::Burden);
    }
    let mut participants = BTreeMap::from([(actor, view.container(actor)?.revision)]);
    for change in &changes {
        participants.insert(
            change.after.id,
            change.before.as_ref().map_or(0, |i| i.revision),
        );
        if let ItemPlace::Contained { container, .. } = change.after.place {
            for ancestor in view.ancestry(container)? {
                let revision = view
                    .items
                    .iter()
                    .find(|i| i.id == ancestor)
                    .map(|i| i.revision)
                    .or_else(|| {
                        view.containers
                            .iter()
                            .find(|c| c.id == ancestor)
                            .map(|c| c.revision)
                    })
                    .ok_or(E::MissingContainer)?;
                participants.insert(ancestor, revision);
            }
        }
    }
    if changes.len() > 1024 || participants.len() > 1024 {
        return Err(E::Capacity);
    }
    Ok(InventoryProposal {
        changes,
        participants: participants.into_iter().collect(),
        actor_burden,
        requires_pickup_motion: false,
    })
}

/// A Refuse interaction examines an existing owned item. This is a real read
/// reservation over its ancestry, with no fabricated item mutation.
pub fn propose_npc_inspection(
    actor: EntityId,
    item: EntityId,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, E> {
    view.validate()?;
    let inspected = view.item(item)?;
    if view.owner(inspected)? != Some(actor) || inspected.trade_reserved || inspected.active_pet {
        return Err(E::AccessDenied);
    }
    let mut participants = BTreeMap::new();
    for id in view.ancestry(item)? {
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
            .ok_or(E::MissingContainer)?;
        participants.insert(id, revision);
    }
    let actor_burden = view.items.iter().try_fold(0u64, |n, i| {
        if view.owner(i)? == Some(actor) {
            n.checked_add(u64::from(i.stack) * u64::from(i.unit_burden))
                .ok_or(E::Overflow)
        } else {
            Ok(n)
        }
    })?;
    Ok(InventoryProposal {
        changes: vec![],
        participants: participants.into_iter().collect(),
        actor_burden,
        requires_pickup_motion: false,
    })
}
