//! Pet-device Structure consumption is independent of inventory stack quantity.
use crate::{InventoryProposal, InventoryView, ItemChange};
use bace_gameplay_api::InventoryRejection as Error;
use bace_types::EntityId;
pub fn propose_pet_charge(
    actor: EntityId,
    item: EntityId,
    unlimited: bool,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    let before = view.item(item)?;
    if view.owner(before)? != Some(actor) {
        return Err(Error::OwnershipMismatch);
    }
    if before.trade_reserved || before.active_pet || before.is_container {
        return Err(Error::Busy);
    }
    let mut after = before.clone();
    after.structure = match before.structure {
        Some(value) => Some(value.checked_sub(1).ok_or(Error::Requirements)?),
        None if unlimited => None,
        None => return Err(Error::Requirements),
    };
    after.active_pet = true;
    crate::propose_item_changes(
        actor,
        vec![ItemChange {
            before: Some(before.clone()),
            after,
        }],
        view,
    )
}
pub fn propose_pet_release(
    actor: EntityId,
    item: EntityId,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    let before = view.item(item)?;
    if view.owner(before)? != Some(actor) {
        return Err(Error::OwnershipMismatch);
    }
    if before.trade_reserved || !before.active_pet {
        return Err(Error::InvalidState);
    }
    let mut after = before.clone();
    after.active_pet = false;
    crate::propose_item_changes(
        actor,
        vec![ItemChange {
            before: Some(before.clone()),
            after,
        }],
        view,
    )
}
