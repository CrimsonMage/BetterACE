//! ACE Player_Inventory.cs:2243–2838, pin recorded in docs/baselines.toml.
//! Fresh factory state is mandatory: a source clone is not a split template.
use crate::{InventoryAuthority, InventoryItem, InventoryProposal, InventoryView, ItemPlace};
use bace_gameplay_api::{InventoryRejection as Error, InventoryRequest};

/// Immutable content and world-owner evidence prepared before reserving a split.
/// These flags are never accepted from client packets.
#[derive(Clone, Debug)]
pub struct StackSplitPreparation {
    pub fresh: InventoryItem,
    pub source_stackable: bool,
    pub source_stuck: bool,
    pub source_vendor: bool,
    pub destination_corpse: bool,
}

pub fn propose_stack_split(
    request: InventoryRequest,
    authority: InventoryAuthority,
    prepared: StackSplitPreparation,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    let (id, destination, world, wield) = match request {
        InventoryRequest::SplitToContainer {
            item, container, ..
        } => (item, Some(container), false, false),
        InventoryRequest::SplitToWorld { item, .. } => (item, None, true, false),
        InventoryRequest::SplitToWield { item, .. } => (item, Some(authority.actor), false, true),
        _ => return Err(Error::InvalidState),
    };
    let source = view.item(id)?;
    if !prepared.source_stackable || source.is_container {
        return Err(Error::InvalidState);
    }
    if !world && (prepared.source_stuck || !(0x80000000..=0xfffffffe).contains(&id.0)) {
        return Err(Error::AccessDenied);
    }
    if prepared.destination_corpse && !world && !wield {
        return Err(Error::AccessDenied);
    }
    if wield && source.attuned {
        return Err(Error::Attuned);
    }
    let source_owned = view.owner(source)? == Some(authority.actor);
    let destination_owned = destination
        .map(|id| {
            view.container_owner(id)
                .map(|owner| owner == Some(authority.actor))
        })
        .transpose()?
        .unwrap_or(false);
    if prepared.source_vendor && source_owned != destination_owned {
        return Err(Error::AccessDenied);
    }
    // ACE checks acquisition burden against the entire source, before splitting.
    if !source_owned && destination_owned {
        let mut burden = u64::from(source.stack) * u64::from(source.unit_burden);
        for item in view.items {
            if item.place != ItemPlace::Removed && view.owner(item)? == Some(authority.actor) {
                burden = burden
                    .checked_add(u64::from(item.stack) * u64::from(item.unit_burden))
                    .ok_or(Error::Overflow)?;
            }
        }
        if burden > view.container(authority.actor)?.burden_limit {
            return Err(Error::Burden);
        }
    }
    let mut proposal =
        crate::actions::propose_inventory_inner(request, authority, view, Some(prepared.fresh))?;
    proposal.requires_pickup_motion |= source_owned != destination_owned;
    Ok(proposal)
}
