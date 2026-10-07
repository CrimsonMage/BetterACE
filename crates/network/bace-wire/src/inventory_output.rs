//! Pinned ACE inventory and player trade event layouts. Success events must only
//! be supplied by authoritative gameplay after its required durable commit.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::{GameEventType as Event, GameMessageOpcode};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerEntry {
    pub object_id: u32,
    pub container_type: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryEvent<'a> {
    PutInWorld {
        object_id: u32,
    },
    PutInContainer {
        object_id: u32,
        container_id: u32,
        placement: i32,
        container_type: u32,
    },
    Wield {
        object_id: u32,
        location: u32,
    },
    ViewContents {
        container_id: u32,
        items: &'a [ContainerEntry],
    },
    CloseContainer {
        container_id: u32,
    },
    SaveFailed {
        item_id: u32,
        error: u32,
    },
    RegisterTrade {
        initiator_id: u32,
        partner_id: u32,
    },
    AddToTrade {
        object_id: u32,
        side: u32,
    },
    AcceptTrade {
        who: u32,
    },
    DeclineTrade {
        who: u32,
    },
    ResetTrade {
        who: u32,
    },
    CloseTrade {
        reason: u32,
    },
    ClearTradeAcceptance,
    TradeFailure {
        object_id: u32,
        reason: u32,
    },
}
impl InventoryEvent<'_> {
    pub fn encode(
        &self,
        object_id: u32,
        sequence: u32,
        max_items: usize,
        max_message_bytes: usize,
    ) -> Result<Vec<u8>, WireError> {
        let event = match self {
            Self::PutInWorld { .. } => Event::InventoryPutObjectIn3D,
            Self::PutInContainer { .. } => Event::InventoryPutObjInContainer,
            Self::Wield { .. } => Event::WieldObject,
            Self::ViewContents { .. } => Event::ViewContents,
            Self::CloseContainer { .. } => Event::CloseGroundContainer,
            Self::SaveFailed { .. } => Event::InventoryServerSaveFailed,
            Self::RegisterTrade { .. } => Event::RegisterTrade,
            Self::AddToTrade { .. } => Event::AddToTrade,
            Self::AcceptTrade { .. } => Event::AcceptTrade,
            Self::DeclineTrade { .. } => Event::DeclineTrade,
            Self::ResetTrade { .. } => Event::ResetTrade,
            Self::CloseTrade { .. } => Event::CloseTrade,
            Self::ClearTradeAcceptance => Event::ClearTradeAcceptance,
            Self::TradeFailure { .. } => Event::TradeFailure,
        };
        let mut writer = message_writer(GameMessageOpcode::GameEvent);
        for value in [object_id, sequence, event.0] {
            writer.u32(value);
        }
        match self {
            Self::PutInWorld { object_id } => writer.u32(*object_id),
            Self::PutInContainer {
                object_id,
                container_id,
                placement,
                container_type,
            } => {
                for value in [
                    *object_id,
                    *container_id,
                    *placement as u32,
                    *container_type,
                ] {
                    writer.u32(value);
                }
            }
            Self::Wield {
                object_id,
                location,
            } => {
                writer.u32(*object_id);
                writer.u32(*location);
            }
            Self::ViewContents {
                container_id,
                items,
            } => {
                if items.len() > max_items || items.len() > u32::MAX as usize {
                    return Err(WireError::LimitExceeded);
                }
                // Projection supplies ACE's stable placement ordering.
                writer.u32(*container_id);
                writer.u32(items.len() as u32);
                for item in *items {
                    writer.u32(item.object_id);
                    writer.u32(item.container_type);
                }
            }
            Self::CloseContainer { container_id } => writer.u32(*container_id),
            Self::SaveFailed { item_id, error } => {
                writer.u32(*item_id);
                writer.u32(*error);
            }
            Self::RegisterTrade {
                initiator_id,
                partner_id,
            } => {
                writer.u32(*initiator_id);
                writer.u32(*partner_id);
                writer.u64(0);
            }
            Self::AddToTrade { object_id, side } => {
                writer.u32(*object_id);
                writer.u32(*side);
                writer.u32(0);
            }
            Self::AcceptTrade { who } | Self::DeclineTrade { who } | Self::ResetTrade { who } => {
                writer.u32(*who)
            }
            Self::CloseTrade { reason } => writer.u32(*reason),
            Self::ClearTradeAcceptance => {}
            Self::TradeFailure { object_id, reason } => {
                writer.u32(*object_id);
                writer.u32(*reason);
            }
        }
        if writer.position() > max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(writer.into_bytes())
    }
}
