//! Charge accounting for ACE `Healer.DoHealing`. Admission, the skill contest,
//! vitals and durable adoption remain with the simulation owner.
use crate::{InventoryItem, InventoryProposal, InventoryView, ItemChange, ItemPlace};
use bace_gameplay_api::InventoryRejection as Error;
use bace_types::EntityId;

/// Freeze the kit's source-visible charge/value transition for one actual
/// healing attempt. ACE spends a use after the motion, even when the skill
/// contest fails. Callers must not invoke this for pre-motion rejection.
pub fn propose_healing_kit_charge(
    actor: EntityId,
    kit: EntityId,
    max_structure: Option<u32>,
    template_value: Option<u32>,
    view: InventoryView<'_>,
) -> Result<InventoryProposal, Error> {
    view.validate()?;
    let before = view.item(kit)?;
    if view.owner(before)? != Some(actor) {
        return Err(Error::OwnershipMismatch);
    }
    if before.trade_reserved || before.active_pet || before.is_container || before.stack != 1 {
        return Err(Error::Busy);
    }
    let mut after: InventoryItem = before.clone();
    let uses = before.structure.ok_or(Error::Requirements)?;
    let maximum = max_structure
        .filter(|n| *n > 0)
        .ok_or(Error::Requirements)?;
    if uses == 0 || uses > maximum {
        return Err(Error::Requirements);
    }
    let remaining = uses - 1;
    after.structure = Some(remaining);
    let source_value = template_value.ok_or(Error::Requirements)?;
    after.unit_value = before.unit_value.saturating_sub(source_value / maximum);
    if remaining == 0 {
        after.stack = 0;
        after.place = ItemPlace::Removed;
    }
    after.revision = before.revision.checked_add(1).ok_or(Error::Overflow)?;
    crate::propose_item_changes(
        actor,
        vec![ItemChange {
            before: Some(before.clone()),
            after,
        }],
        view,
    )
}
