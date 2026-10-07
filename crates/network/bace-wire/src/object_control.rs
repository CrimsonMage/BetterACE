//! Fixed-size object lifecycle, parentage and state messages from pinned ACE.
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode as Op;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectControl {
    Delete {
        object_id: u32,
        instance_sequence: u16,
    },
    PlayerCreate {
        object_id: u32,
    },
    SetState {
        object_id: u32,
        state: u32,
        instance_sequence: u16,
        state_sequence: u16,
    },
    Parent {
        parent_id: u32,
        child_id: u32,
        parent_location: u32,
        placement: u32,
        instance_sequence: u16,
        position_sequence: u16,
    },
    Pickup {
        object_id: u32,
        instance_sequence: u16,
        position_sequence: u16,
    },
    InventoryRemove {
        object_id: u32,
    },
    StackSize {
        object_id: u32,
        sequence: u8,
        stack_size: u32,
        value: u32,
    },
    Teleport {
        sequence: u16,
    },
}
impl ObjectControl {
    pub fn encode(self) -> Vec<u8> {
        let opcode = match self {
            Self::Delete { .. } => Op::ObjectDelete,
            Self::PlayerCreate { .. } => Op::PlayerCreate,
            Self::SetState { .. } => Op::SetState,
            Self::Parent { .. } => Op::ParentEvent,
            Self::Pickup { .. } => Op::PickupEvent,
            Self::InventoryRemove { .. } => Op::InventoryRemoveObject,
            Self::StackSize { .. } => Op::SetStackSize,
            Self::Teleport { .. } => Op::PlayerTeleport,
        };
        let mut writer = message_writer(opcode);
        match self {
            Self::Delete {
                object_id,
                instance_sequence,
            } => {
                writer.u32(object_id);
                writer.u16(instance_sequence);
                writer.align4();
            }
            Self::PlayerCreate { object_id } | Self::InventoryRemove { object_id } => {
                writer.u32(object_id)
            }
            Self::SetState {
                object_id,
                state,
                instance_sequence,
                state_sequence,
            } => {
                writer.u32(object_id);
                writer.u32(state);
                writer.u16(instance_sequence);
                writer.u16(state_sequence);
            }
            Self::Parent {
                parent_id,
                child_id,
                parent_location,
                placement,
                instance_sequence,
                position_sequence,
            } => {
                for v in [parent_id, child_id, parent_location, placement] {
                    writer.u32(v);
                }
                writer.u16(instance_sequence);
                writer.u16(position_sequence);
            }
            Self::Pickup {
                object_id,
                instance_sequence,
                position_sequence,
            } => {
                writer.u32(object_id);
                writer.u16(instance_sequence);
                writer.u16(position_sequence);
            }
            Self::StackSize {
                object_id,
                sequence,
                stack_size,
                value,
            } => {
                writer.bytes(&[sequence]);
                writer.u32(object_id);
                writer.u32(stack_size);
                writer.u32(value);
            }
            Self::Teleport { sequence } => {
                writer.u16(sequence);
                writer.align4();
            }
        }
        writer.into_bytes()
    }
}
