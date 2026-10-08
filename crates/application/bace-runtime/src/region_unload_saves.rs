//! Cold region checkpoint composition. Save every held tree before eviction;
//! each bounded batch retains the same epoch, operation ID and bytes on retry.
use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeInput};
use crate::placement_saves::{PendingPlacementSave, PlacementResolution};
use crate::saves::{SaveHandle, SaveSubmitError};
use bace_inventory::{InventoryProposal, ItemChange, ItemPlace};
use bace_persistence::SaveAck;
use bace_simulation::{RegionUnloadReceipt, RegionUnloadTicket};
use bace_storage_codec::{CorpseSaveV5, ItemSaveV5};
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct RegionItemSource {
    pub item: FrozenInventoryItem,
    /// Preserve original death identity and wall-clock expiry, never restart decay.
    pub corpse: Option<CorpseSaveV5>,
}
pub struct PendingRegionUnloadSave {
    batches: Vec<PendingPlacementSave>,
    next: usize,
    receipt: RegionUnloadReceipt,
    acknowledgments: Vec<SaveAck>,
    blocked: bool,
}
#[derive(Debug)]
pub enum RegionUnloadResolution {
    Progress,
    Committed {
        receipt: RegionUnloadReceipt,
        acknowledgments: Vec<SaveAck>,
    },
    Uncertain(String),
    /// The world must retain the held tree, including already committed batches.
    Blocked(String),
}
impl PendingRegionUnloadSave {
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.blocked {
            return Err(SaveSubmitError::Invalid);
        }
        self.batches
            .get_mut(self.next)
            .ok_or(SaveSubmitError::Invalid)?
            .submit(saves)
    }
    pub fn poll(&mut self) -> Option<RegionUnloadResolution> {
        match self.batches.get_mut(self.next)?.poll()? {
            PlacementResolution::Committed(acks) => {
                self.acknowledgments.extend(acks);
                self.next += 1;
                if self.next == self.batches.len() {
                    Some(RegionUnloadResolution::Committed {
                        receipt: self.receipt.clone(),
                        acknowledgments: std::mem::take(&mut self.acknowledgments),
                    })
                } else {
                    Some(RegionUnloadResolution::Progress)
                }
            }
            PlacementResolution::Uncertain(error) => Some(RegionUnloadResolution::Uncertain(error)),
            PlacementResolution::Rejected(error) => {
                self.blocked = true;
                Some(RegionUnloadResolution::Blocked(error.to_string()))
            }
        }
    }
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }
    pub fn committed_batches(&self) -> usize {
        self.next
    }
}

pub fn freeze_region_unload(
    world_epoch: u64,
    ticket: &RegionUnloadTicket,
    sources: &[RegionItemSource],
) -> Result<PendingRegionUnloadSave, String> {
    if world_epoch == 0
        || world_epoch > i64::MAX as u64
        || ticket.operation == 0
        || ticket.epoch == 0
        || ticket.items.is_empty()
        || ticket.items.len() > 4096
        || sources.len() != ticket.items.len()
    {
        return Err("invalid region unload bounds".into());
    }
    let mut by_id = BTreeMap::new();
    let mut source_by_id = BTreeMap::new();
    for item in &ticket.items {
        if by_id.insert(item.item.id, item).is_some() {
            return Err("duplicate region item".into());
        }
    }
    for source in sources {
        if source_by_id
            .insert(EntityId(source.item.entity.object_id), source)
            .is_some()
        {
            return Err("duplicate region source".into());
        }
    }
    if by_id.keys().ne(source_by_id.keys()) {
        return Err("region source identities differ".into());
    }
    let mut trees: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
    for &id in by_id.keys() {
        let mut ancestor = id;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(ancestor) || visited.len() > 128 {
                return Err("region container cycle/depth".into());
            }
            let item = by_id.get(&ancestor).ok_or("region tree lacks ancestor")?;
            match item.item.place {
                ItemPlace::World => {
                    let pos = item.position.ok_or("region root lacks accepted position")?;
                    if pos.cell >> 16 != u32::from(ticket.landblock) {
                        return Err("region root position mismatch".into());
                    }
                    trees.entry(ancestor).or_default().push(id);
                    break;
                }
                ItemPlace::Contained { container, .. } => {
                    ancestor = container;
                }
                ItemPlace::Removed => return Err("removed item in unload tree".into()),
            }
        }
    }
    let mut groups: Vec<Vec<EntityId>> = vec![];
    for tree in trees.into_values() {
        if tree.len() > 1024 {
            return Err("region tree exceeds atomic save capacity".into());
        }
        if groups.last().is_none_or(|g| g.len() + tree.len() > 1024) {
            groups.push(vec![]);
        }
        groups.last_mut().expect("created group").extend(tree);
    }
    let mut batches = Vec::with_capacity(groups.len());
    let mut retained_bytes = 0usize;
    for (index, ids) in groups.into_iter().enumerate() {
        let operation_id = format!(
            "region-unload:{world_epoch}:{}:{}:{}:{index}",
            ticket.landblock, ticket.epoch, ticket.operation
        );
        let mut changes = vec![];
        let mut frozen = vec![];
        let mut positions = BTreeMap::new();
        let mut transient = vec![];
        for id in ids {
            let captured = by_id[&id];
            let source = source_by_id[&id];
            if source.item.entity.mutation_revision > captured.item.revision
                || source.item.entity.state.weenie_id != captured.item.template
                || source.item.persisted_version < 0
                || (source.item.persisted_version == 0) != captured.transient
            {
                return Err("region item baseline revision/identity mismatch".into());
            }
            if captured.registry_revision.is_none()
                && (!captured.enchantments.is_empty() || !source.item.enchantments.is_empty())
            {
                return Err("region enchantment owner missing".into());
            }
            let mut saved = source.item.clone();
            saved.enchantments = captured
                .enchantments
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            let mut before = captured.item.clone();
            before.revision = saved.entity.mutation_revision;
            before.structure = saved
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|v| v.id == 92)
                .map(|v| u32::try_from(v.value))
                .transpose()
                .map_err(|_| "negative saved structure")?;
            before.stack = saved
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|v| v.id == 12)
                .map_or(Ok(1), |v| u32::try_from(v.value))
                .map_err(|_| "negative saved stack")?;
            if let Some(pos) = captured.position {
                let position = bace_content::Position {
                    obj_cell_id: pos.cell,
                    position_x: pos.origin[0],
                    position_y: pos.origin[1],
                    position_z: pos.origin[2],
                    rotation_w: pos.rotation[0],
                    rotation_x: pos.rotation[1],
                    rotation_y: pos.rotation[2],
                    rotation_z: pos.rotation[3],
                };
                positions.insert(id.0, position);
            }
            if captured.transient {
                transient.push(id.0);
            }
            changes.push(ItemChange {
                before: Some(before),
                after: captured.item.clone(),
            });
            frozen.push(saved);
        }
        let proposal = InventoryProposal {
            changes,
            participants: vec![],
            actor_burden: 0,
            requires_pickup_motion: false,
        };
        let input = InventoryFreezeInput {
            operation_id: &operation_id,
            proposal: &proposal,
            items: &frozen,
            other_snapshots: &[],
            leases: &[],
            storage_views: &[],
            admitted_positions: &positions,
        };
        let mut operation = if transient.is_empty() {
            bace_persistence::WorldPlacementOperation {
                world_epoch,
                inventory: crate::game_inventory::freeze_inventory(input)
                    .map_err(|e| e.to_string())?,
            }
        } else {
            crate::game_inventory::freeze_generated_inventory(input, &transient, world_epoch)
                .map_err(|e| e.to_string())?
        };
        for snapshot in &mut operation.inventory.snapshots {
            let captured = by_id[&EntityId(snapshot.object_id)];
            let source = source_by_id[&EntityId(snapshot.object_id)];
            match (&captured.corpse, &source.corpse) {
                (None, None) => (),
                (Some(live), Some(old)) => {
                    if old.corpse.entity != source.item.entity
                        || old.placement
                            != *source
                                .item
                                .placement
                                .as_ref()
                                .ok_or("corpse placement missing")?
                        || old.corpse.owner != live.owner.map(|id| id.0)
                        || old.source != Some(live.source.0)
                        || old.operation != Some(live.operation)
                        || live.template != captured.item.template
                    {
                        return Err("region corpse baseline mismatch".into());
                    }
                    let item = ItemSaveV5::decode(&snapshot.bytes).map_err(|e| e.to_string())?;
                    let mut corpse = old.clone();
                    corpse.corpse.entity = item.entity.clone();
                    corpse.placement = item.placement.clone();
                    corpse.enchantments = item.previous.previous.enchantments;
                    snapshot.bytes = corpse.encode().map_err(|e| e.to_string())?;
                }
                _ => return Err("region corpse type mismatch".into()),
            }
        }
        for snapshot in &operation.inventory.snapshots {
            retained_bytes = retained_bytes
                .checked_add(snapshot.bytes.len())
                .ok_or("region save byte overflow")?;
            if retained_bytes > 64 * 1024 * 1024 {
                return Err("region save byte capacity".into());
            }
        }
        batches.push(
            PendingPlacementSave::new_world(operation)
                .map_err(|e| format!("region pending save: {e:?}"))?,
        );
    }
    Ok(PendingRegionUnloadSave {
        batches,
        next: 0,
        blocked: false,
        acknowledgments: vec![],
        receipt: RegionUnloadReceipt {
            operation: ticket.operation,
            landblock: ticket.landblock,
            epoch: ticket.epoch,
            revisions: ticket
                .items
                .iter()
                .map(|i| (i.item.id, i.item.revision, i.registry_revision))
                .collect(),
        },
    })
}

#[cfg(test)]
#[path = "region_unload_saves_tests.rs"]
mod tests;
