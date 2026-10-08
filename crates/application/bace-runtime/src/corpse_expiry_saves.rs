//! Frozen corpse expiry uses the same world-epoch CAS journal as generated
//! retirement. Timeouts retain the exact operation; no in-memory deletion occurs
//! before every tombstone and corpse metadata row commit together.
use crate::{
    game_inventory::{
        FrozenInventoryItem, InventoryFreezeInput, freeze_generated_inventory, freeze_inventory,
    },
    placement_saves::PendingPlacementSave,
};
use bace_persistence::WorldPlacementOperation;
use bace_simulation::{CorpseExpiryTicket, InventoryReceipt};
use bace_storage_codec::{CorpseSaveV5, ItemPlacementV2, ItemSaveV5};
use std::collections::BTreeMap;
/// Source WorldObject_Decay spill pose, calculated in f32 before geometry admission.
pub fn corpse_spill_position(
    source: bace_gameplay_api::GeneratorLocation,
    scale: Option<f64>,
) -> Result<bace_content::Position, String> {
    let scale = scale.unwrap_or(1.);
    if !scale.is_finite()
        || scale <= 0.
        || scale > f64::from(f32::MAX)
        || source
            .origin
            .iter()
            .chain(source.rotation.iter())
            .any(|v| !v.is_finite())
    {
        return Err("invalid corpse spill pose/scale".into());
    }
    let z = source.origin[2] + 0.05f32 * scale as f32;
    if !z.is_finite() {
        return Err("corpse spill height overflow".into());
    }
    Ok(bace_content::Position {
        obj_cell_id: source.cell,
        position_x: source.origin[0],
        position_y: source.origin[1],
        position_z: z,
        rotation_x: source.rotation[0],
        rotation_y: source.rotation[1],
        rotation_z: source.rotation[2],
        rotation_w: source.rotation[3],
    })
}
pub struct CorpseExpiryFreezeInput<'a> {
    pub world_epoch: u64,
    pub ticket: &'a CorpseExpiryTicket,
    pub corpse: &'a CorpseSaveV5,
    pub items: &'a [FrozenInventoryItem],
    pub unix_seconds: i64,
    pub spill_positions: &'a BTreeMap<u32, bace_content::Position>,
}
pub struct FrozenCorpseExpiry {
    pub save: PendingPlacementSave,
    pub receipt: InventoryReceipt,
}
pub fn freeze_corpse_expiry(
    input: CorpseExpiryFreezeInput<'_>,
) -> Result<FrozenCorpseExpiry, String> {
    let t = input.ticket;
    if input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || t.death_operation == 0
        || input.corpse.operation != Some(t.death_operation)
        || input.corpse.source.is_none()
        || t.inventory.operation == 0
        || t.inventory.actor != t.corpse
        || input.corpse.corpse.entity.object_id != t.corpse.0
        || input.unix_seconds < input.corpse.corpse.expires_at
        || input.unix_seconds < 0
        || t.inventory.proposal.changes.is_empty()
    {
        return Err("invalid corpse expiry identity/deadline/proposal".into());
    }
    input.corpse.validate().map_err(|e| e.to_string())?;
    let player = input
        .corpse
        .source
        .is_some_and(|id| (0x5000_0001..=0x5fff_ffff).contains(&id));
    let roots: Vec<_> = t.inventory.proposal.changes.iter().filter_map(|c| c.before.as_ref())
        .filter(|b| matches!(b.place, bace_inventory::ItemPlace::Contained { container, .. } if container == t.corpse))
        .map(|b| b.id).collect();
    if player && !roots.is_empty() {
        let intent = t
            .spill
            .as_ref()
            .ok_or("player corpse requires spill intent")?;
        let expected: std::collections::BTreeSet<_> = roots.iter().copied().collect();
        if expected != intent.roots.iter().copied().collect()
            || intent.roots.len() != expected.len()
            || input.spill_positions.len() != expected.len()
            || expected
                .iter()
                .any(|id| !input.spill_positions.contains_key(&id.0))
        {
            return Err("corpse spill identity mismatch".into());
        }
        for (id, position) in input.spill_positions {
            let source = input
                .items
                .iter()
                .find(|i| i.entity.object_id == *id)
                .ok_or("spill source missing")?;
            let scale = source
                .entity
                .state
                .properties
                .floats
                .iter()
                .find(|p| p.id == 39)
                .map(|p| p.value);
            if *position != corpse_spill_position(intent.position, scale)? {
                return Err("corpse spill source position mismatch".into());
            }
        }
        for c in &t.inventory.proposal.changes {
            let before = c
                .before
                .as_ref()
                .ok_or("corpse spill existing item missing")?;
            let expected = if c.after.id == t.corpse {
                bace_inventory::ItemPlace::Removed
            } else if roots.contains(&c.after.id) {
                bace_inventory::ItemPlace::World
            } else {
                before.place
            };
            if c.after.place != expected {
                return Err("corpse spill descendant placement mismatch".into());
            }
        }
    } else if t.spill.is_some()
        || !input.spill_positions.is_empty()
        || t.inventory
            .proposal
            .changes
            .iter()
            .any(|c| c.after.place != bace_inventory::ItemPlace::Removed)
    {
        return Err("invalid corpse tombstone tree".into());
    }
    let root = t
        .inventory
        .proposal
        .changes
        .iter()
        .find(|c| c.after.id == t.corpse)
        .ok_or("missing corpse tombstone")?;
    if root.before.as_ref().is_none_or(|b| {
        b.revision != input.corpse.corpse.entity.mutation_revision
            || b.place != bace_inventory::ItemPlace::World
    }) {
        return Err("stale corpse expiry snapshot".into());
    }
    let frozen = input
        .items
        .iter()
        .find(|i| i.entity.object_id == t.corpse.0)
        .ok_or("missing corpse freeze source")?;
    if frozen.entity != input.corpse.corpse.entity
        || frozen.placement.as_ref() != Some(&input.corpse.placement)
        || frozen.enchantments != input.corpse.enchantments
    {
        return Err("corpse expiry metadata mismatch".into());
    }
    let id = format!(
        "corpse-expiry:{}:{}",
        input.world_epoch, t.inventory.operation
    );
    let mut items = input.items.to_vec();
    for id in input.spill_positions.keys() {
        let source = items
            .iter_mut()
            .find(|i| i.entity.object_id == *id)
            .ok_or("spill source missing")?;
        crate::game_inventory::set(&mut source.entity.state.properties.ints, 65, 101);
    }
    let freeze = InventoryFreezeInput {
        operation_id: &id,
        proposal: &t.inventory.proposal,
        items: &items,
        other_snapshots: &[],
        leases: &[],
        storage_views: &[],
        admitted_positions: input.spill_positions,
    };
    let mut operation = if t.transient.is_empty() {
        WorldPlacementOperation {
            world_epoch: input.world_epoch,
            inventory: freeze_inventory(freeze).map_err(|e| e.to_string())?,
        }
    } else {
        let transient: Vec<_> = t.transient.iter().map(|id| id.0).collect();
        freeze_generated_inventory(freeze, &transient, input.world_epoch)
            .map_err(|e| e.to_string())?
    };
    // Corpse spills are not player acquisitions. Preserve source quest
    // GeneratorID instead of applying the generic first-acquisition clearing.
    for change in t
        .inventory
        .proposal
        .changes
        .iter()
        .filter(|c| c.after.place != bace_inventory::ItemPlace::Removed)
    {
        let source = items
            .iter()
            .find(|i| i.entity.object_id == change.after.id.0)
            .ok_or("spill source missing")?;
        let snapshot = operation
            .inventory
            .snapshots
            .iter_mut()
            .find(|s| s.object_id == change.after.id.0)
            .ok_or("spill snapshot missing")?;
        let mut saved = ItemSaveV5::decode(&snapshot.bytes).map_err(|e| e.to_string())?;
        saved
            .entity
            .state
            .properties
            .instance_ids
            .retain(|p| p.id != 6);
        if let Some(generator) = source
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 6)
        {
            crate::game_inventory::set(
                &mut saved.entity.state.properties.instance_ids,
                6,
                generator.value,
            );
        }
        snapshot.bytes = saved.encode().map_err(|e| e.to_string())?;
    }
    let snapshot = operation
        .inventory
        .snapshots
        .iter_mut()
        .find(|s| s.object_id == t.corpse.0)
        .ok_or("missing frozen corpse tombstone")?;
    let item = ItemSaveV5::decode(&snapshot.bytes).map_err(|e| e.to_string())?;
    if item.placement != ItemPlacementV2::Removed {
        return Err("corpse expiry must remove placement".into());
    }
    let mut corpse = input.corpse.clone();
    corpse.corpse.entity = item.previous.previous.previous.entity;
    corpse.placement = item.previous.previous.previous.placement;
    corpse.enchantments = item.previous.previous.enchantments;
    snapshot.bytes = corpse.encode().map_err(|e| e.to_string())?;
    let receipt = InventoryReceipt {
        operation: t.inventory.operation,
        revisions: t
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    Ok(FrozenCorpseExpiry {
        save: PendingPlacementSave::new_world(operation).map_err(|e| e.to_string())?,
        receipt,
    })
}
