//! Gear rows and the complete source inventory fence enter the same journal
//! transaction. Existing item DTOs retain metadata; no gameplay struct is saved.
#[cfg(test)]
mod tests;
use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeInput};
use bace_inventory::{InventoryProposal, ItemChange};
use bace_persistence::{NpcStageOperation, PlacementOperation, SaveSnapshot};
use bace_simulation::NpcSourceInventorySnapshot;
use bace_storage_codec::{
    NpcWorkflowSaveV3, SaveCodecError, npc_workflow_v3::NpcSourceInventoryV3,
};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn joined(operation: &NpcStageOperation) -> bool {
    let Ok(checkpoint) = NpcWorkflowSaveV3::decode_or_migrate(&operation.workflow.checkpoint)
    else {
        return false;
    };
    let Some(proof) = checkpoint.inventory.as_ref() else {
        return true;
    };
    proof.source_persisted_version > 0
        && operation.inventory.snapshots.iter().any(|s| {
            s.object_id == checkpoint.source
                && s.mutation_revision == proof.source_mutation_revision
                && s.expected_version.checked_add(1) == Some(proof.source_persisted_version)
        })
        && proof.items.iter().all(|item| {
            item.persisted_version > 0
                && operation.inventory.snapshots.iter().any(|s| {
                    s.object_id == item.id
                        && s.mutation_revision == item.revision
                        && s.expected_version.checked_add(1) == Some(item.persisted_version)
                })
        })
}
pub(crate) struct FrozenNpcSourceInventory {
    pub source: u32,
    pub ticket: u64,
    pub fence: NpcSourceInventoryV3,
    operation: PlacementOperation,
    baseline: Option<bace_persistence::StoredAggregate>,
    authored: std::sync::Arc<bace_content::WeenieV1>,
    template_revision: u64,
    root_item: Option<bace_simulation::RegionUnloadItem>,
    root_after: std::sync::OnceLock<SaveSnapshot>,
}
impl FrozenNpcSourceInventory {
    fn freeze_root(&self, checkpoint: &NpcWorkflowSaveV3) -> Result<SaveSnapshot, SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("NPC source aggregate identity/revision");
        let properties = checkpoint
            .live_properties
            .clone()
            .map(super::source_state::thaw_properties)
            .transpose()?
            .ok_or_else(invalid)?;
        let state = crate::npc_recovery::overlay_npc_qualities(&self.authored, &properties)
            .map_err(|_| invalid())?;
        let expected_version = self.baseline.as_ref().map_or(0, |b| b.persisted_version);
        let (mutation_revision, bytes) = if let Some(root) = &self.root_item {
            let baseline = self.baseline.as_ref().ok_or_else(invalid)?;
            let mut saved =
                bace_storage_codec::ItemSaveV5::decode_or_migrate(&baseline.bytes, None)?;
            if saved.entity.object_id != self.source
                || saved.entity.state.weenie_id != state.weenie_id
                || saved.entity.mutation_revision >= root.item.revision
            {
                return Err(invalid());
            }
            saved.previous.previous.previous.entity.state = state;
            saved.previous.previous.previous.entity.mutation_revision = root.item.revision;
            saved.previous.previous.enchantments = self.fence.source_enchantments.clone();
            saved.previous.previous.previous.placement = match root.item.place {
                bace_inventory::ItemPlace::World => {
                    let p = root.position.ok_or_else(invalid)?;
                    bace_storage_codec::ItemPlacementV2::World(bace_content::Position {
                        obj_cell_id: p.cell,
                        position_x: p.origin[0],
                        position_y: p.origin[1],
                        position_z: p.origin[2],
                        rotation_w: p.rotation[0],
                        rotation_x: p.rotation[1],
                        rotation_y: p.rotation[2],
                        rotation_z: p.rotation[3],
                    })
                }
                bace_inventory::ItemPlace::Contained {
                    container,
                    slot,
                    equipped,
                } => bace_storage_codec::ItemPlacementV2::Contained {
                    container: container.0,
                    slot,
                    pack_slot: root.item.pack_slot,
                    equipped,
                },
                bace_inventory::ItemPlace::Removed => return Err(invalid()),
            };
            // Inventory owns these values even when source scalar qualities have a
            // separate revision. Preserve the exact held graph's accepted counters.
            let ints = &mut saved
                .previous
                .previous
                .previous
                .entity
                .state
                .properties
                .ints;
            for (id, value) in [(12, Some(root.item.stack)), (92, root.item.structure)] {
                if let Some(value) = value {
                    let value = i32::try_from(value).map_err(|_| invalid())?;
                    if let Some(row) = ints.iter_mut().find(|p| p.id == id) {
                        row.value = value;
                    } else {
                        ints.push(bace_content::Property { id, value });
                    }
                }
            }
            (root.item.revision, saved.encode()?)
        } else {
            let previous = self
                .baseline
                .as_ref()
                .map(|b| bace_storage_codec::EntitySaveV1::decode_item(&b.bytes))
                .transpose()?;
            if previous
                .as_ref()
                .is_some_and(|p| p.object_id != self.source || p.state.weenie_id != state.weenie_id)
            {
                return Err(invalid());
            }
            let revision = previous
                .as_ref()
                .map_or(0, |p| p.mutation_revision)
                .checked_add(1)
                .ok_or_else(invalid)?;
            let saved = bace_storage_codec::EntitySaveV1 {
                object_id: self.source,
                template_revision: self.template_revision,
                mutation_revision: revision,
                state,
            };
            (revision, saved.encode_item()?)
        };
        Ok(SaveSnapshot {
            object_id: self.source,
            mutation_revision,
            expected_version,
            bytes,
        })
    }
    pub(crate) fn snapshots(&self) -> &[SaveSnapshot] {
        &self.operation.snapshots
    }
    pub(crate) fn root_snapshot(&self) -> Option<&SaveSnapshot> {
        self.root_after.get()
    }
    pub(crate) fn root_is_item(&self) -> bool {
        self.root_item.is_some()
    }
    pub(crate) fn attach(&self, operation: &mut NpcStageOperation) -> Result<(), SaveCodecError> {
        let invalid = || SaveCodecError::Invalid("NPC source inventory joint stage");
        let mut checkpoint = NpcWorkflowSaveV3::decode_or_migrate(&operation.workflow.checkpoint)?;
        if checkpoint.source != self.source {
            return Err(invalid());
        }
        let mut before = self.fence.clone();
        for item in &mut before.items {
            item.persisted_version = 0;
        }
        let mut recorded = checkpoint.inventory.clone().ok_or_else(invalid)?;
        recorded.source_persisted_version = 0;
        recorded.source_mutation_revision = 0;
        if recorded != before && recorded != self.fence {
            return Err(invalid());
        }
        let root = self.freeze_root(&checkpoint)?;
        if let Some(old) = self.root_after.get() {
            if old != &root {
                return Err(invalid());
            }
        } else {
            self.root_after.set(root.clone()).map_err(|_| invalid())?;
        }
        let mut fence = self.fence.clone();
        fence.source_persisted_version =
            root.expected_version.checked_add(1).ok_or_else(invalid)?;
        fence.source_mutation_revision = root.mutation_revision;
        let mut rows = operation.inventory.snapshots.clone();
        let mut changes = operation.inventory.changes.clone();
        if self.root_is_item() {
            let saved = bace_storage_codec::ItemSaveV5::decode(&root.bytes)?;
            let expected = if root.expected_version == 0 {
                None
            } else {
                Some(crate::game_inventory::durable(
                    &bace_storage_codec::ItemSaveV5::decode_or_migrate(
                        &self.baseline.as_ref().ok_or_else(invalid)?.bytes,
                        None,
                    )?
                    .placement,
                ))
            };
            let change = bace_persistence::PlacementChange {
                item: self.source,
                expected,
                destination: crate::game_inventory::durable(&saved.placement),
            };
            if let Some(old) = changes.iter().find(|c| c.item == self.source) {
                if old != &change {
                    return Err(invalid());
                }
            } else {
                changes.push(change);
            }
        }
        for row in std::iter::once(&root).chain(self.operation.snapshots.iter()) {
            if let Some(old) = rows.iter().find(|v| v.object_id == row.object_id) {
                if old != row {
                    return Err(invalid());
                }
            } else {
                rows.push(row.clone());
            }
        }
        for change in &self.operation.changes {
            if let Some(old) = changes.iter().find(|old| old.item == change.item) {
                if old != change {
                    return Err(invalid());
                }
            } else {
                changes.push(*change);
            }
        }
        let mut participants: BTreeSet<_> =
            operation.inventory.participants.iter().copied().collect();
        participants.extend(self.operation.participants.iter().copied());
        if rows.len() > 1024
            || changes.len() > 1024
            || participants.len() > 1024
            || rows
                .iter()
                .try_fold(0usize, |n, r| n.checked_add(r.bytes.len()))
                .is_none_or(|n| n > 64 * 1024 * 1024)
        {
            return Err(invalid());
        }
        checkpoint.inventory = Some(fence);
        let bytes = checkpoint.encode()?;
        operation.inventory.snapshots = rows;
        operation.inventory.changes = changes;
        operation.inventory.participants = participants.into_iter().collect();
        operation.workflow.checkpoint = bytes;
        Ok(())
    }
}
pub(crate) fn freeze_source_inventory(
    snapshot: &NpcSourceInventorySnapshot,
    sources: &[FrozenInventoryItem],
    world_epoch: u64,
    registration: &crate::npc_sources::PreparedNpcRegistration,
) -> Result<FrozenNpcSourceInventory, String> {
    if sources.len() != snapshot.items.len() || sources.len() > 1024 {
        return Err("NPC source inventory bounds".into());
    }
    let mut indexed = BTreeMap::new();
    for source in sources {
        if indexed.insert(source.entity.object_id, source).is_some() {
            return Err("NPC source item duplicate".into());
        }
    }
    let mut changes = Vec::new();
    let mut frozen = Vec::new();
    let mut transient = Vec::new();
    for captured in &snapshot.items {
        let baseline = indexed
            .get(&captured.item.id.0)
            .ok_or("NPC source item baseline missing")?;
        if baseline.entity.state.weenie_id != captured.item.template || baseline.corpse.is_some() {
            return Err("NPC source item metadata mismatch".into());
        }
        let mut saved: FrozenInventoryItem = (**baseline).clone();
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
            .find(|p| p.id == 92)
            .map(|p| u32::try_from(p.value))
            .transpose()
            .map_err(|_| "NPC saved structure")?;
        before.stack = saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .map_or(Ok(1), |p| u32::try_from(p.value))
            .map_err(|_| "NPC saved stack")?;
        changes.push(ItemChange {
            before: Some(before),
            after: captured.item.clone(),
        });
        if captured.transient {
            transient.push(captured.item.id.0);
        }
        frozen.push(saved);
    }
    let proposal = InventoryProposal {
        changes,
        participants: vec![],
        actor_burden: 0,
        requires_pickup_motion: false,
    };
    let positions = BTreeMap::new();
    let input = InventoryFreezeInput {
        operation_id: "npc-source-inventory-prepared",
        proposal: &proposal,
        items: &frozen,
        other_snapshots: &[],
        leases: &[],
        storage_views: &[],
        admitted_positions: &positions,
    };
    let mut operation = if transient.is_empty() {
        crate::game_inventory::freeze_inventory(input).map_err(|e| e.to_string())?
    } else {
        crate::game_inventory::freeze_generated_inventory(input, &transient, world_epoch)
            .map_err(|e| e.to_string())?
            .inventory
    };
    operation.participants.push(snapshot.source.0);
    operation.participants.sort_unstable();
    operation.participants.dedup();
    let mut fence =
        super::source_state::freeze_inventory(snapshot.clone()).map_err(|e| e.to_string())?;
    for item in &mut fence.items {
        let row = operation
            .snapshots
            .iter()
            .find(|r| r.object_id == item.id)
            .ok_or("NPC source row not frozen")?;
        item.persisted_version = row
            .expected_version
            .checked_add(1)
            .ok_or("NPC source row version overflow")?;
    }
    Ok(FrozenNpcSourceInventory {
        source: snapshot.source.0,
        ticket: snapshot.ticket,
        fence,
        operation,
        baseline: registration.baseline.clone(),
        authored: registration.source.authored.clone(),
        template_revision: registration.generation.revision(),
        root_item: snapshot.root_item.clone(),
        root_after: std::sync::OnceLock::new(),
    })
}
