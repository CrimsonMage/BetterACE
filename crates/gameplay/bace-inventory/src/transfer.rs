//! Authoritative item transfer validation. Inspired by pinned ACE
//! Player_Inventory.cs; durability-before-success is BetterACE hardening.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemState {
    pub item: u32,
    pub owner: u32,
    pub revision: u64,
    pub stack: u32,
    pub unit_burden: u32,
    pub attuned: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerState {
    pub owner: u32,
    pub revision: u64,
    pub slots_used: u32,
    pub slots_max: u32,
    pub burden: u64,
    pub burden_max: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    InvalidIdentity,
    WrongOwner,
    InvalidCount,
    Attuned,
    Full,
    Burden,
    Overflow,
    InvalidContainer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferProposal {
    pub item: ItemState,
    pub destination: ContainerState,
    pub next_item_revision: u64,
    pub next_container_revision: u64,
    pub transferred_burden: u64,
}
/// Propose an entire-stack transfer after caller-established range/access checks.
/// Reservations and durable operation IDs belong to the simulation/persistence
/// coordinator. The proposal grants no ownership and must be CAS-committed before
/// client success. Stack splitting/merging needs its own multi-item transaction.
pub fn propose_transfer(
    item: ItemState,
    expected_owner: u32,
    destination: ContainerState,
    crosses_character_ownership: bool,
) -> Result<TransferProposal, TransferError> {
    if item.item == 0
        || destination.owner == 0
        || item.item == destination.owner
        || item.owner == destination.owner
    {
        return Err(TransferError::InvalidIdentity);
    }
    if item.owner != expected_owner {
        return Err(TransferError::WrongOwner);
    }
    if item.stack == 0 {
        return Err(TransferError::InvalidCount);
    }
    if item.attuned && crosses_character_ownership {
        return Err(TransferError::Attuned);
    }
    if destination.slots_used > destination.slots_max || destination.burden > destination.burden_max
    {
        return Err(TransferError::InvalidContainer);
    }
    if destination.slots_used == destination.slots_max {
        return Err(TransferError::Full);
    }
    let transferred_burden = u64::from(item.stack) * u64::from(item.unit_burden);
    if destination
        .burden
        .checked_add(transferred_burden)
        .ok_or(TransferError::Overflow)?
        > destination.burden_max
    {
        return Err(TransferError::Burden);
    }
    let next_item_revision = item
        .revision
        .checked_add(1)
        .ok_or(TransferError::Overflow)?;
    let next_container_revision = destination
        .revision
        .checked_add(1)
        .ok_or(TransferError::Overflow)?;
    Ok(TransferProposal {
        item,
        destination,
        next_item_revision,
        next_container_revision,
        transferred_burden,
    })
}
