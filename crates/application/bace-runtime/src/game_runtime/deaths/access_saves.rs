//! Exact corpse-rights checkpoint. The active viewer is transient; permittee
//! consumption and IsLooted are persisted before the simulation adopts them.
use crate::region_unload_saves::RegionItemSource;
use bace_persistence::{
    DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot, WorldPlacementOperation,
};
use bace_simulation::CorpseAccessDecision;
use bace_storage_codec::{
    CorpseSaveV5, ItemPlacementV2, SaveCodecError, validate_corpse_transition_v5,
};
use bace_types::EntityId;

pub(super) fn freeze(
    epoch: u64,
    correlation: u64,
    corpse: EntityId,
    actor: EntityId,
    decision: CorpseAccessDecision,
    source: &RegionItemSource,
) -> Result<Option<(WorldPlacementOperation, CorpseSaveV5)>, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("corpse access source/receipt");
    if epoch == 0
        || epoch > i64::MAX as u64
        || correlation == 0
        || corpse.0 == 0
        || actor.0 == 0
        || source.item.entity.object_id != corpse.0
        || source.item.persisted_version < 0
        || source.item.persisted_version == i64::MAX
    {
        return Err(invalid());
    }
    let before = source.corpse.as_ref().ok_or_else(invalid)?;
    if before.corpse.entity.object_id != corpse.0
        || before.corpse.entity != source.item.entity
        || source.item.placement.as_ref() != Some(&before.placement)
    {
        return Err(invalid());
    }
    let mut after = before.clone();
    match decision {
        CorpseAccessDecision::Open {
            consume_permit: true,
        } => {
            let index = match after.access.permittees.binary_search(&actor.0) {
                Ok(_) => return Err(invalid()),
                Err(index) if after.access.permittees.len() < 1024 => index,
                Err(_) => return Err(invalid()),
            };
            after.access.permittees.insert(index, actor.0);
        }
        CorpseAccessDecision::Close { mark_looted: true } => {
            if after.access.looted {
                return Err(invalid());
            }
            after.access.looted = true;
        }
        CorpseAccessDecision::Open {
            consume_permit: false,
        }
        | CorpseAccessDecision::Close { mark_looted: false } => return Ok(None),
        CorpseAccessDecision::Denied(_) => return Err(invalid()),
    }
    after.corpse.entity.mutation_revision = before
        .corpse
        .entity
        .mutation_revision
        .checked_add(1)
        .ok_or_else(invalid)?;
    validate_corpse_transition_v5(before, &after)?;
    let placement = match &before.placement {
        ItemPlacementV2::World(position) => {
            let cell = position.obj_cell_id;
            if source.item.persisted_version == 0 {
                vec![PlacementChange {
                    item: corpse.0,
                    expected: None,
                    destination: DurableItemPlace::World { cell },
                }]
            } else {
                Vec::new()
            }
        }
        _ => return Err(invalid()),
    };
    let operation = PlacementOperation {
        operation_id: format!("corpse-access:{epoch}:{}:{correlation}", corpse.0),
        snapshots: vec![SaveSnapshot {
            object_id: corpse.0,
            mutation_revision: after.corpse.entity.mutation_revision,
            expected_version: source.item.persisted_version,
            bytes: after.encode()?,
        }],
        participants: vec![corpse.0],
        leases: Vec::new(),
        changes: placement,
        storage_views: Vec::new(),
    };
    Ok(Some((
        WorldPlacementOperation {
            world_epoch: epoch,
            inventory: operation,
        },
        after,
    )))
}
