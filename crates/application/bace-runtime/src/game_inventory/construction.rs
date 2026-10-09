//! The companion is evidence about this exact graph, never a second graph owner.
use super::*;

pub(super) fn validate(
    items: &[FrozenInventoryItem],
    proposal: &InventoryProposal,
) -> Result<(), InventoryFreezeError> {
    let changed: BTreeMap<_, _> = proposal.changes.iter().map(|c| (c.after.id.0, c)).collect();
    if changed.len() != proposal.changes.len()
        || items
            .iter()
            .filter_map(|i| i.construction.as_ref())
            .try_fold(0usize, |sum, c| {
                sum.checked_add(c.equipment_order.len() + c.death_roster.len())
            })
            .is_none_or(|count| count > 65536)
    {
        return Err(InventoryFreezeError::Capacity);
    }
    let by_id: BTreeMap<_, _> = items
        .iter()
        .map(|item| (item.entity.object_id, item))
        .collect();
    if by_id.len() != items.len() {
        return Err(InventoryFreezeError::Identity);
    }
    for item in items {
        let Some(construction) = &item.construction else {
            if matches!(item.entity.state.weenie_type, 10 | 12 | 15 | 61 | 69 | 71) {
                return Err(InventoryFreezeError::Identity);
            }
            continue;
        };
        let root = item.entity.object_id;
        construction.validate(root, item.entity.state.weenie_type)?;
        for after in [false, true] {
            let placement = |id: u32| -> Option<ItemPlacementV2> {
                if after && let Some(change) = changed.get(&id) {
                    return match change.after.place {
                        ItemPlace::Contained {
                            container,
                            slot,
                            equipped,
                        } => Some(ItemPlacementV2::Contained {
                            container: container.0,
                            slot,
                            pack_slot: change.after.pack_slot,
                            equipped,
                        }),
                        ItemPlace::Removed => Some(ItemPlacementV2::Removed),
                        ItemPlace::World => None,
                    };
                }
                by_id.get(&id).and_then(|i| i.placement.clone())
            };
            // Tombstones retain constructor provenance but no surviving child graph.
            if after && matches!(placement(root), Some(ItemPlacementV2::Removed)) {
                continue;
            }
            for row in &construction.death_roster {
                if !matches!(placement(row.entity), Some(ItemPlacementV2::Contained { container, .. }) if container == row.parent.unwrap_or(root))
                    || !death_child_belongs_to(root, row.entity, &by_id, &placement)
                {
                    return Err(InventoryFreezeError::Identity);
                }
            }
            let expected: BTreeSet<_> = construction.equipment_order.iter().copied().collect();
            let actual: BTreeSet<_> = by_id.keys().copied().filter(|&id| matches!(placement(id), Some(ItemPlacementV2::Contained { container, equipped, .. }) if container == root && equipped != 0)).collect();
            if expected != actual {
                return Err(InventoryFreezeError::Identity);
            }
        }
    }
    Ok(())
}

fn death_child_belongs_to(
    root: u32,
    child: u32,
    items: &BTreeMap<u32, &FrozenInventoryItem>,
    placement: &impl Fn(u32) -> Option<ItemPlacementV2>,
) -> bool {
    let mut current = child;
    for _ in 0..64 {
        if current == root {
            return child != root;
        }
        // A nested Creature root may itself be selected for its parent's
        // source death roster. Its children belong to its own companion.
        if current != child
            && items
                .get(&current)
                .is_some_and(|item| item.construction.is_some())
        {
            return false;
        }
        let Some(ItemPlacementV2::Contained { container, .. }) = placement(current) else {
            return false;
        };
        current = container;
    }
    false
}

#[cfg(test)]
#[path = "construction/tests.rs"]
mod tests;
