//! Epoch-fenced retirement of stale generator forests before initial spawning.
use crate::{generated_recovery::HeldGeneratedWorldTree, placement_saves::PendingPlacementSave};
use bace_persistence::{
    DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot, WorldPlacementOperation,
};
use bace_storage_codec::ItemPlacementV2;
pub(super) fn freeze(tree: &HeldGeneratedWorldTree) -> Result<PendingPlacementSave, String> {
    if tree.snapshots.is_empty() || tree.snapshots.len() > 1024 {
        return Err("generated recovery tree exceeds atomic capacity".into());
    }
    let mut op = PlacementOperation {
        operation_id: format!("generated-recovery:{}:{}", tree.world_epoch, tree.root),
        snapshots: vec![],
        participants: vec![],
        leases: vec![],
        changes: vec![],
        storage_views: vec![],
    };
    for source in &tree.snapshots {
        let mut decoded = super::world_items::decode(source)?;
        let id = source.aggregate.object_id;
        let revision = decoded
            .item
            .entity
            .mutation_revision
            .checked_add(1)
            .ok_or("generated recovery revision overflow")?;
        decoded.item.previous.previous.entity.mutation_revision = revision;
        decoded.item.previous.placement = ItemPlacementV2::Removed;
        decoded
            .item
            .previous
            .entity
            .state
            .properties
            .instance_ids
            .retain(|p| ![1, 2, 3].contains(&p.id));
        decoded
            .item
            .previous
            .entity
            .state
            .properties
            .positions
            .retain(|p| p.id != 1);
        decoded
            .item
            .previous
            .entity
            .state
            .properties
            .ints
            .retain(|p| ![10, 53].contains(&p.id));
        let bytes = if let Some(mut corpse) = decoded.corpse {
            corpse.previous.previous.corpse.entity = decoded.item.previous.previous.entity;
            corpse.previous.previous.placement = ItemPlacementV2::Removed;
            corpse.encode().map_err(|e| e.to_string())?
        } else {
            decoded.item.encode().map_err(|e| e.to_string())?
        };
        op.participants.push(id);
        op.changes.push(PlacementChange {
            item: id,
            expected: Some(source.placement),
            destination: DurableItemPlace::Removed,
        });
        op.snapshots.push(SaveSnapshot {
            object_id: id,
            mutation_revision: revision,
            expected_version: source.aggregate.persisted_version,
            bytes,
        });
    }
    PendingPlacementSave::new_world(WorldPlacementOperation {
        world_epoch: tree.world_epoch,
        inventory: op,
    })
    .map_err(|e| format!("generated recovery freeze: {e:?}"))
}
