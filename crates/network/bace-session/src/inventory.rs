//! Bounded raw inventory proposals; runtime supplies immutable factory/geometry
//! inputs and simulation owns permissions, replay, placement and durability.
use crate::{DispatchError, SessionState};
use bace_gameplay_api::{ActionContext, InventoryRequest};
use bace_types::EntityId;
use bace_wire::{GameActionEnvelope, InventoryAction, opcode::GameActionType as Op};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedInventory {
    pub context: ActionContext,
    pub request: InventoryRequest,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_inventory(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    maximum: usize,
) -> Result<DispatchedInventory, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope = GameActionEnvelope::decode(bytes, maximum).map_err(DispatchError::Wire)?;
    if !matches!(
        envelope.action,
        Op::PutItemInContainer
            | Op::GetAndWieldItem
            | Op::DropItem
            | Op::StackableMerge
            | Op::StackableSplitToContainer
            | Op::StackableSplitTo3D
            | Op::StackableSplitToWield
    ) {
        return Err(DispatchError::UnsupportedAction(envelope.action.0));
    }
    let value = bace_wire::InventoryRequest::decode(envelope.action, envelope.payload, maximum, 0)
        .map_err(DispatchError::Wire)?;
    let request = match value.action {
        InventoryAction::PutInContainer {
            item_id,
            container_id,
            placement,
        } => InventoryRequest::Move {
            item: EntityId(item_id),
            container: EntityId(container_id),
            placement,
        },
        InventoryAction::Wield { item_id, location } => InventoryRequest::Equip {
            item: EntityId(item_id),
            location,
        },
        InventoryAction::Drop(item) => InventoryRequest::Drop {
            item: EntityId(item),
        },
        InventoryAction::Merge {
            source_id,
            target_id,
            amount,
        } => InventoryRequest::Merge {
            source: EntityId(source_id),
            target: EntityId(target_id),
            amount,
        },
        InventoryAction::SplitToContainer {
            stack_id,
            container_id,
            placement,
            amount,
        } => InventoryRequest::SplitToContainer {
            item: EntityId(stack_id),
            container: EntityId(container_id),
            placement,
            amount,
        },
        InventoryAction::SplitToWorld { stack_id, amount } => InventoryRequest::SplitToWorld {
            item: EntityId(stack_id),
            amount,
        },
        InventoryAction::SplitToWield {
            stack_id,
            location,
            amount,
        } => InventoryRequest::SplitToWield {
            item: EntityId(stack_id),
            location,
            amount,
        },
        _ => return Err(DispatchError::UnsupportedAction(envelope.action.0)),
    };
    binding.sequence = envelope.sequence;
    Ok(DispatchedInventory {
        context: binding,
        request,
        ignored_trailing_bytes: value.trailing_bytes,
    })
}
