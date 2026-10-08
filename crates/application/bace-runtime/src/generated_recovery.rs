//! Bounded cold recovery classification, not a world-item restore service.
//! Durable generated remnants must be reconciled under the current world epoch
//! before fresh generator initialization; ordinary dropped items remain restorable.
use bace_persistence::{DurableItemPlace, InventoryLoadLimits, LocatedSnapshot};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedRecoveryError {
    Bounds,
    Identity,
    Placement,
    Ancestry,
    Snapshot,
}
#[derive(Debug)]
pub struct HeldGeneratedWorldTree {
    pub root: u32,
    pub world_epoch: u64,
    /// Every item and its retained GeneratorId (IID 6), including descendants.
    pub generator_links: Vec<(u32, u32)>,
    /// Original bytes, persisted versions and relational placements are retained.
    pub snapshots: Vec<LocatedSnapshot>,
}
#[derive(Debug)]
pub struct GeneratedWorldRecovery {
    pub restore_candidates: Vec<LocatedSnapshot>,
    pub held_for_retirement: Vec<HeldGeneratedWorldTree>,
}
/// A held tree must not be admitted alongside a new generator incarnation. This
/// function neither deletes snapshots nor acknowledges an epoch-fenced tombstone.
/// A mixed ordinary/generated tree is held whole so no descendant is orphaned.
pub fn classify_generated_world_trees(
    snapshots: Vec<LocatedSnapshot>,
    world_epoch: u64,
    limits: InventoryLoadLimits,
) -> Result<GeneratedWorldRecovery, (GeneratedRecoveryError, Vec<LocatedSnapshot>)> {
    let (roots, links) = match validate(&snapshots, world_epoch, limits) {
        Ok(result) => result,
        Err(error) => return Err((error, snapshots)),
    };
    let mut held: BTreeMap<_, _> = links
        .into_iter()
        .map(|(root, generator_links)| {
            (
                root,
                HeldGeneratedWorldTree {
                    root,
                    world_epoch,
                    generator_links,
                    snapshots: Vec::new(),
                },
            )
        })
        .collect();
    let mut restore_candidates = Vec::new();
    for snapshot in snapshots {
        let root = roots[&snapshot.aggregate.object_id];
        if let Some(tree) = held.get_mut(&root) {
            tree.snapshots.push(snapshot);
        } else {
            restore_candidates.push(snapshot);
        }
    }
    restore_candidates.sort_by_key(|snapshot| (snapshot.depth, snapshot.aggregate.object_id));
    for tree in held.values_mut() {
        tree.snapshots
            .sort_by_key(|snapshot| (snapshot.depth, snapshot.aggregate.object_id));
    }
    Ok(GeneratedWorldRecovery {
        restore_candidates,
        held_for_retirement: held.into_values().collect(),
    })
}
type Roots = BTreeMap<u32, u32>;
type Links = BTreeMap<u32, Vec<(u32, u32)>>;
fn validate(
    snapshots: &[LocatedSnapshot],
    epoch: u64,
    limits: InventoryLoadLimits,
) -> Result<(Roots, Links), GeneratedRecoveryError> {
    use GeneratedRecoveryError as E;
    if epoch == 0
        || epoch > i64::MAX as u64
        || limits.max_items > 4096
        || limits.max_depth == 0
        || limits.max_depth > 64
        || limits.max_total_bytes > 64 * 1024 * 1024
        || snapshots.len() > limits.max_items
    {
        return Err(E::Bounds);
    }
    let mut indexed = BTreeMap::new();
    let mut generator_ids = BTreeMap::new();
    let mut constructed_roots = BTreeSet::new();
    let mut bytes = 0usize;
    for snapshot in snapshots {
        let aggregate = &snapshot.aggregate;
        if aggregate.object_id == 0
            || aggregate.object_id == u32::MAX
            || aggregate.persisted_version <= 0
            || indexed.insert(aggregate.object_id, snapshot).is_some()
        {
            return Err(E::Identity);
        }
        bytes = bytes.checked_add(aggregate.bytes.len()).ok_or(E::Bounds)?;
        if aggregate.bytes.is_empty()
            || aggregate.bytes.len() > 2 * 1024 * 1024 + 52
            || bytes > limits.max_total_bytes
        {
            return Err(E::Bounds);
        }
        let decoded = crate::region_service::world_items::decode_payload(snapshot)
            .map_err(|_| E::Snapshot)?
            .item;
        if decoded.entity.object_id != aggregate.object_id {
            return Err(E::Identity);
        }
        if crate::game_inventory::durable(&decoded.placement) != snapshot.placement {
            return Err(E::Placement);
        }
        if let Some(property) = decoded
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|property| property.id == 6)
        {
            if property.value == 0 || property.value == u32::MAX {
                return Err(E::Identity);
            }
            generator_ids.insert(aggregate.object_id, property.value);
            if matches!(snapshot.placement, DurableItemPlace::Contained { .. })
                && matches!(decoded.entity.state.weenie_type, 10 | 15)
                && decoded
                    .construction
                    .as_ref()
                    .is_some_and(|construction| construction.origin.generator == property.value)
            {
                constructed_roots.insert(aggregate.object_id);
            }
        }
    }
    let mut roots = BTreeMap::new();
    let mut links: Links = BTreeMap::new();
    for snapshot in snapshots {
        let id = snapshot.aggregate.object_id;
        let mut current = snapshot;
        let mut seen = BTreeSet::new();
        let mut depth = 0usize;
        loop {
            if !seen.insert(current.aggregate.object_id) {
                return Err(E::Ancestry);
            }
            match current.placement {
                DurableItemPlace::World { .. } => {
                    if current.depth != 0 || usize::from(snapshot.depth) != depth {
                        return Err(E::Ancestry);
                    }
                    let root = current.aggregate.object_id;
                    roots.insert(id, root);
                    if let Some(generator) = generator_ids.get(&id)
                        && seen.is_disjoint(&constructed_roots)
                    {
                        links.entry(root).or_default().push((id, *generator));
                    }
                    break;
                }
                DurableItemPlace::Contained { container, .. } => {
                    depth = depth.checked_add(1).ok_or(E::Bounds)?;
                    if depth > limits.max_depth {
                        return Err(E::Bounds);
                    }
                    let parent = indexed.get(&container).ok_or(E::Ancestry)?;
                    if parent.depth.checked_add(1) != Some(current.depth) {
                        return Err(E::Ancestry);
                    }
                    current = parent;
                }
                _ => return Err(E::Placement),
            }
        }
    }
    for entries in links.values_mut() {
        entries.sort_unstable();
    }
    Ok((roots, links))
}
