//! The corpse, player rewards and allegiance counters use one placement journal.
use crate::death_saves::{DeathFreezeInput, freeze_native_death};
use bace_persistence::{AllegiancePlacementOperation, StoredAllegiance};
use bace_storage_codec::SaveCodecError;

pub fn freeze_shared_death(
    input: DeathFreezeInput<'_>,
    world_epoch: u64,
    nodes: &[StoredAllegiance],
    metadata: &[StoredAllegiance],
) -> Result<AllegiancePlacementOperation, SaveCodecError> {
    freeze_shared_death_with_items(input, world_epoch, nodes, metadata, &[])
}
pub fn freeze_shared_death_with_items(
    input: DeathFreezeInput<'_>,
    world_epoch: u64,
    nodes: &[StoredAllegiance],
    metadata: &[StoredAllegiance],
    items: &[(u64, bace_persistence::PlacementOperation)],
) -> Result<AllegiancePlacementOperation, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("shared death allegiance ticket");
    let ticket = input.proposal.social.as_ref().ok_or_else(invalid)?;
    let mut allegiance = crate::allegiance_saves::freeze_allegiance(
        crate::allegiance_saves::AllegianceFreezeInput {
            world_epoch,
            operation: ticket.operation,
            patch: &ticket.patch,
            stored_nodes: nodes,
            stored_metadata: metadata,
            players: &[],
            leases: input.leases,
        },
    )?;
    let placement = freeze_native_death(input)?;
    let mut placement =
        crate::item_reward_join::join_allegiance_item_operations(placement, ticket, items)?;
    allegiance.operation_id = placement.operation_id.clone();
    placement
        .participants
        .extend(allegiance.nodes.iter().map(|n| n.character));
    placement
        .participants
        .extend(allegiance.metadata.iter().map(|n| n.character));
    placement.participants.sort_unstable();
    placement.participants.dedup();
    Ok(AllegiancePlacementOperation {
        placement,
        allegiance,
        world_epoch,
        workflow: None,
    })
}
