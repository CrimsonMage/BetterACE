//! Freeze already-authorized inventory proposals without losing item metadata.
//! Physical drop poses are admitted by the world owner, never decoded from client input.
mod cold;
mod construction;
use bace_content::{Position, Property};
use bace_inventory::{InventoryProposal, ItemPlace};
use bace_persistence::{
    CharacterLease, DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot,
    StorageViewFence,
};
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV2};
pub use cold::decode_inventory_item;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub struct FrozenInventoryItem {
    pub corpse: Option<Box<bace_storage_codec::CorpseSaveV5>>,
    pub construction: Option<bace_storage_codec::FrozenCreatureConstructionV1>,
    pub source_destination: Option<u8>,
    pub enchantments: Vec<bace_storage_codec::FrozenEnchantmentV1>,
    pub entity: EntitySaveV1,
    pub placement: Option<ItemPlacementV2>,
    pub persisted_version: i64,
}
pub struct InventoryFreezeInput<'a> {
    pub operation_id: &'a str,
    pub proposal: &'a InventoryProposal,
    /// Include unchanged item ancestors of destinations as well as changed items.
    /// A container absent from this item graph is an unowned static root unless
    /// it is identified by an online character lease.
    pub items: &'a [FrozenInventoryItem],
    pub other_snapshots: &'a [SaveSnapshot],
    pub leases: &'a [CharacterLease],
    pub storage_views: &'a [StorageViewFence],
    pub admitted_positions: &'a BTreeMap<u32, Position>,
}
#[derive(Debug, thiserror::Error)]
pub enum InventoryFreezeError {
    #[error("inventory snapshot identity/revision mismatch")]
    Identity,
    #[error("missing admitted drop position for {0}")]
    MissingPosition(u32),
    #[error("inventory frozen numeric overflow")]
    Overflow,
    #[error("inventory freeze capacity")]
    Capacity,
    #[error(transparent)]
    Save(#[from] bace_storage_codec::SaveCodecError),
}
pub fn freeze_inventory(
    input: InventoryFreezeInput<'_>,
) -> Result<PlacementOperation, InventoryFreezeError> {
    freeze_inventory_inner(input, &BTreeSet::new())
}

/// Freeze a reserved acquisition of previously unsaved generated items. The
/// live before-places still have to match, while their durable expected places
/// are absent. Include every transient descendant in the same proposal.
/// The supplied epoch is part of the immutable idempotency fingerprint.
pub fn freeze_generated_inventory(
    input: InventoryFreezeInput<'_>,
    transient_items: &[u32],
    world_epoch: u64,
) -> Result<bace_persistence::WorldPlacementOperation, InventoryFreezeError> {
    let ids: BTreeSet<_> = transient_items.iter().copied().collect();
    if world_epoch == 0
        || world_epoch > i64::MAX as u64
        || ids.is_empty()
        || ids.len() != transient_items.len()
        || ids.len() > 1024
        || ids.iter().any(|id| {
            !input
                .proposal
                .changes
                .iter()
                .any(|change| change.after.id.0 == *id && change.before.is_some())
        })
    {
        return Err(InventoryFreezeError::Identity);
    }
    Ok(bace_persistence::WorldPlacementOperation {
        world_epoch,
        inventory: freeze_inventory_inner(input, &ids)?,
    })
}

fn freeze_inventory_inner(
    input: InventoryFreezeInput<'_>,
    transient_items: &BTreeSet<u32>,
) -> Result<PlacementOperation, InventoryFreezeError> {
    if input.operation_id.is_empty()
        || input.operation_id.len() > 128
        || input.proposal.changes.len() > 1024
        || input.items.len() > 4096
        || input.other_snapshots.len() > 1024
    {
        return Err(InventoryFreezeError::Capacity);
    }
    construction::validate(input.items, input.proposal)?;
    let mut snapshot_ids = BTreeSet::new();
    for snapshot in input.other_snapshots {
        if snapshot.expected_version < 0
            || snapshot.expected_version == i64::MAX
            || !snapshot_ids.insert(snapshot.object_id)
        {
            return Err(InventoryFreezeError::Identity);
        }
    }
    let mut snapshots = input.other_snapshots.to_vec();
    let mut changes = Vec::with_capacity(input.proposal.changes.len());
    let mut seen = BTreeSet::new();
    for change in &input.proposal.changes {
        if !seen.insert(change.after.id.0) {
            return Err(InventoryFreezeError::Identity);
        }
        let source = input
            .items
            .iter()
            .find(|i| i.entity.object_id == change.after.id.0)
            .ok_or(InventoryFreezeError::Identity)?;
        if source.entity.state.weenie_id != change.after.template
            || source.persisted_version < 0
            || source.persisted_version == i64::MAX
            || change
                .before
                .as_ref()
                .is_some_and(|before| source.entity.mutation_revision != before.revision)
            || change.before.is_none() && source.persisted_version != 0
        {
            return Err(InventoryFreezeError::Identity);
        }
        let transient = transient_items.contains(&change.after.id.0);
        if transient && source.persisted_version != 0 {
            return Err(InventoryFreezeError::Identity);
        }
        let stored_structure = source
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 92)
            .map(|p| u32::try_from(p.value).map_err(|_| InventoryFreezeError::Identity))
            .transpose()?;
        if change
            .before
            .as_ref()
            .is_some_and(|before| before.structure != stored_structure)
        {
            return Err(InventoryFreezeError::Identity);
        }
        if let Some(before) = &change.before {
            if !matches_place(before.place, before.pack_slot, source.placement.as_ref()) {
                return Err(InventoryFreezeError::Identity);
            }
        } else if source.placement.is_some() {
            return Err(InventoryFreezeError::Identity);
        }
        let placement = match change.after.place {
            ItemPlace::Contained {
                container,
                slot,
                equipped,
            } => ItemPlacementV2::Contained {
                container: container.0,
                slot,
                pack_slot: change.after.pack_slot,
                equipped,
            },
            ItemPlace::World => {
                let position = input
                    .admitted_positions
                    .get(&change.after.id.0)
                    .cloned()
                    .or_else(|| match &source.placement {
                        Some(ItemPlacementV2::World(p)) => Some(p.clone()),
                        _ => None,
                    })
                    .ok_or(InventoryFreezeError::MissingPosition(change.after.id.0))?;
                ItemPlacementV2::World(position)
            }
            ItemPlace::Removed => ItemPlacementV2::Removed,
        };
        let mut entity = source.entity.clone();
        entity.mutation_revision = change.after.revision;
        if clear_generator_link(change, &input)? {
            // ACE NotifyOfEvent detaches acquired objects; split creates a fresh
            // factory item. Unowned remainders retain their original generator.
            entity.state.properties.instance_ids.retain(|p| p.id != 6);
        }
        match change.after.structure {
            Some(value) => set(
                &mut entity.state.properties.ints,
                92,
                i32::try_from(value).map_err(|_| InventoryFreezeError::Overflow)?,
            ),
            None => entity.state.properties.ints.retain(|p| p.id != 92),
        }

        // Preserve every unrelated frozen property/spell/appearance field.
        let quantity_changed = change
            .before
            .as_ref()
            .is_none_or(|before| before.stack != change.after.stack);
        if change.after.maximum_stack > 1 || entity.state.properties.ints.iter().any(|p| p.id == 12)
        {
            set(
                &mut entity.state.properties.ints,
                12,
                i32::try_from(change.after.stack).map_err(|_| InventoryFreezeError::Overflow)?,
            );
        }
        if quantity_changed && !change.after.is_container {
            set(
                &mut entity.state.properties.ints,
                5,
                i32::try_from(u64::from(change.after.stack) * u64::from(change.after.unit_burden))
                    .map_err(|_| InventoryFreezeError::Overflow)?,
            );
            set(
                &mut entity.state.properties.ints,
                19,
                i32::try_from(u64::from(change.after.stack) * u64::from(change.after.unit_value))
                    .map_err(|_| InventoryFreezeError::Overflow)?,
            );
        }
        entity
            .state
            .properties
            .instance_ids
            .retain(|p| ![1, 2, 3].contains(&p.id));
        entity
            .state
            .properties
            .ints
            .retain(|p| ![10, 53].contains(&p.id));
        entity.state.properties.positions.retain(|p| p.id != 1);
        match &placement {
            ItemPlacementV2::Contained {
                container,
                slot,
                equipped: 0,
                ..
            } => {
                set(&mut entity.state.properties.instance_ids, 1, *container);
                set(&mut entity.state.properties.instance_ids, 2, *container);
                set(
                    &mut entity.state.properties.ints,
                    53,
                    i32::try_from(*slot).map_err(|_| InventoryFreezeError::Overflow)?,
                );
            }
            ItemPlacementV2::Contained {
                container,
                equipped,
                ..
            } => {
                set(&mut entity.state.properties.instance_ids, 3, *container);
                set(&mut entity.state.properties.ints, 10, *equipped as i32);
            }
            ItemPlacementV2::World(position) => {
                set(&mut entity.state.properties.positions, 1, position.clone())
            }
            ItemPlacementV2::Removed => {}
        }
        let saved = bace_storage_codec::ItemSaveV5 {
            previous: bace_storage_codec::ItemSaveV4 {
                previous: bace_storage_codec::ItemSaveV3 {
                    previous: ItemSaveV2 {
                        entity,
                        placement: placement.clone(),
                    },
                    enchantments: source.enchantments.clone(),
                },
                construction: source.construction.clone(),
            },
            source_destination: source.source_destination,
        };
        let snapshot = SaveSnapshot {
            object_id: saved.entity.object_id,
            mutation_revision: saved.entity.mutation_revision,
            expected_version: source.persisted_version,
            bytes: saved.encode()?,
        };
        if snapshots.iter().any(|s| s.object_id == snapshot.object_id) {
            return Err(InventoryFreezeError::Identity);
        }
        snapshots.push(snapshot);
        changes.push(PlacementChange {
            item: saved.entity.object_id,
            expected: if transient {
                None
            } else {
                source.placement.as_ref().map(durable)
            },
            destination: durable(&placement),
        });
    }
    let mut participants: BTreeSet<_> = input
        .proposal
        .participants
        .iter()
        .map(|(id, _)| id.0)
        .collect();
    participants.extend(snapshots.iter().map(|s| s.object_id));
    participants.extend(input.leases.iter().map(|l| l.character_id));
    for view in input.storage_views {
        participants.insert(view.house);
        participants.insert(view.actor);
    }
    if snapshots.len() > 1024 || participants.len() > 1024 {
        return Err(InventoryFreezeError::Capacity);
    }
    Ok(PlacementOperation {
        operation_id: input.operation_id.into(),
        snapshots,
        participants: participants.into_iter().collect(),
        leases: input.leases.to_vec(),
        changes,
        storage_views: input.storage_views.to_vec(),
    })
}
fn matches_place(view: ItemPlace, pack: bool, stored: Option<&ItemPlacementV2>) -> bool {
    match (view, stored) {
        (ItemPlace::World, Some(ItemPlacementV2::World(_)))
        | (ItemPlace::Removed, Some(ItemPlacementV2::Removed)) => true,
        (
            ItemPlace::Contained {
                container,
                slot,
                equipped,
            },
            Some(ItemPlacementV2::Contained {
                container: c,
                slot: s,
                equipped: e,
                pack_slot,
            }),
        ) => container.0 == *c && slot == *s && equipped == *e && pack == *pack_slot,
        _ => false,
    }
}
pub(crate) fn durable(p: &ItemPlacementV2) -> DurableItemPlace {
    match p {
        ItemPlacementV2::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } => DurableItemPlace::Contained {
            container: *container,
            slot: *slot,
            pack_slot: *pack_slot,
            equipped: *equipped,
        },
        ItemPlacementV2::World(p) => DurableItemPlace::World {
            cell: p.obj_cell_id,
        },
        ItemPlacementV2::Removed => DurableItemPlace::Removed,
    }
}
pub(crate) fn set<T>(properties: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(p) = properties.iter_mut().find(|p| p.id == id) {
        p.value = value
    } else {
        properties.push(Property { id, value });
        properties.sort_by_key(|p| p.id);
    }
}

/// Follow accepted after-placements, then immutable unchanged ancestor snapshots.
/// Timeouts and caller retries reuse the same frozen result; no owner lookup or
/// external I/O can alter the operation fingerprint after this function returns.
fn clear_generator_link(
    change: &bace_inventory::ItemChange,
    input: &InventoryFreezeInput<'_>,
) -> Result<bool, InventoryFreezeError> {
    if change.before.is_none() || change.after.place == ItemPlace::Removed {
        return Ok(true);
    }
    let mut place = change.after.place;
    let mut seen = BTreeSet::from([change.after.id.0]);
    for _ in 0..64 {
        let ItemPlace::Contained { container, .. } = place else {
            return Ok(false);
        };
        if !seen.insert(container.0) {
            return Err(InventoryFreezeError::Identity);
        }
        if input.leases.iter().any(|lease| {
            lease.character_id == container.0
                && lease.state == bace_persistence::OwnershipState::Online
        }) {
            return Ok(true);
        }
        if let Some(parent) = input
            .proposal
            .changes
            .iter()
            .find(|candidate| candidate.after.id == container)
        {
            place = parent.after.place;
            if place == ItemPlace::Removed {
                return Err(InventoryFreezeError::Identity);
            }
            continue;
        }
        let mut parents = input
            .items
            .iter()
            .filter(|candidate| candidate.entity.object_id == container.0);
        let Some(parent) = parents.next() else {
            return Ok(false);
        };
        if parents.next().is_some() {
            return Err(InventoryFreezeError::Identity);
        }
        place = match parent.placement.as_ref() {
            Some(ItemPlacementV2::Contained {
                container,
                slot,
                equipped,
                ..
            }) => ItemPlace::Contained {
                container: bace_types::EntityId(*container),
                slot: *slot,
                equipped: *equipped,
            },
            Some(ItemPlacementV2::World(_)) => ItemPlace::World,
            Some(ItemPlacementV2::Removed) | None => return Err(InventoryFreezeError::Identity),
        };
    }
    Err(InventoryFreezeError::Identity)
}
