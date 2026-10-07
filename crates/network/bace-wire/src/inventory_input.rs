//! Inventory/trade requests are untrusted proposals. Ownership, quantities,
//! account identity and durable success are never decided by these codecs.
use crate::opcode::GameActionType as Op;
use crate::{Reader, WireError};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VendorItemRequest {
    pub amount: i32,
    pub object_id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TradeAcceptance {
    pub partner_id: u32,
    pub trade_stamp: f64,
    pub status: u32,
    pub initiator_id: u32,
    pub initiator_accepts: bool,
    pub partner_accepts: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum InventoryAction {
    PutInContainer {
        item_id: u32,
        container_id: u32,
        placement: i32,
    },
    Wield {
        item_id: u32,
        location: u32,
    },
    Drop(u32),
    Use(u32),
    UseWithTarget {
        source_id: u32,
        target_id: u32,
    },
    Merge {
        source_id: u32,
        target_id: u32,
        amount: i32,
    },
    SplitToContainer {
        stack_id: u32,
        container_id: u32,
        placement: i32,
        amount: i32,
    },
    SplitToWorld {
        stack_id: u32,
        amount: i32,
    },
    SplitToWield {
        stack_id: u32,
        location: u32,
        amount: i32,
    },
    Give {
        target_id: u32,
        item_id: u32,
        amount: i32,
    },
    Buy {
        vendor_id: u32,
        items: Vec<VendorItemRequest>,
    },
    Sell {
        vendor_id: u32,
        items: Vec<VendorItemRequest>,
    },
    OpenTrade(u32),
    CloseTrade,
    AddToTrade {
        item_id: u32,
        slot: u32,
    },
    AcceptTrade(TradeAcceptance),
    DeclineTrade,
    ResetTrade,
    StopViewing(u32),
}
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryRequest {
    pub action: InventoryAction,
    pub trailing_bytes: usize,
}
impl InventoryRequest {
    pub fn decode(
        opcode: Op,
        payload: &[u8],
        max_payload_bytes: usize,
        max_items: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(payload);
        let action = match opcode {
            Op::PutItemInContainer => InventoryAction::PutInContainer {
                item_id: reader.u32()?,
                container_id: reader.u32()?,
                placement: reader.u32()? as i32,
            },
            Op::GetAndWieldItem => InventoryAction::Wield {
                item_id: reader.u32()?,
                location: reader.u32()?,
            },
            Op::DropItem => InventoryAction::Drop(reader.u32()?),
            Op::Use => InventoryAction::Use(reader.u32()?),
            Op::UseWithTarget => InventoryAction::UseWithTarget {
                source_id: reader.u32()?,
                target_id: reader.u32()?,
            },
            Op::StackableMerge => InventoryAction::Merge {
                source_id: reader.u32()?,
                target_id: reader.u32()?,
                amount: reader.u32()? as i32,
            },
            Op::StackableSplitToContainer => InventoryAction::SplitToContainer {
                stack_id: reader.u32()?,
                container_id: reader.u32()?,
                placement: reader.u32()? as i32,
                amount: reader.u32()? as i32,
            },
            Op::StackableSplitTo3D => InventoryAction::SplitToWorld {
                stack_id: reader.u32()?,
                amount: reader.u32()? as i32,
            },
            Op::StackableSplitToWield => InventoryAction::SplitToWield {
                stack_id: reader.u32()?,
                location: reader.u32()?,
                amount: reader.u32()? as i32,
            },
            Op::GiveObjectRequest => InventoryAction::Give {
                target_id: reader.u32()?,
                item_id: reader.u32()?,
                amount: reader.u32()? as i32,
            },
            Op::Buy | Op::Sell => {
                let vendor_id = reader.u32()?;
                let count = reader.u32()? as usize;
                if count > max_items {
                    return Err(WireError::LimitExceeded);
                }
                if count > reader.remaining() / 8 {
                    return Err(WireError::Truncated);
                }
                let items = (0..count)
                    .map(|_| {
                        Ok(VendorItemRequest {
                            amount: reader.u32()? as i32,
                            object_id: reader.u32()?,
                        })
                    })
                    .collect::<Result<Vec<_>, WireError>>()?;
                if opcode == Op::Buy {
                    InventoryAction::Buy { vendor_id, items }
                } else {
                    InventoryAction::Sell { vendor_id, items }
                }
            }
            Op::OpenTradeNegotiations => InventoryAction::OpenTrade(reader.u32()?),
            Op::CloseTradeNegotiations => InventoryAction::CloseTrade,
            Op::AddToTrade => InventoryAction::AddToTrade {
                item_id: reader.u32()?,
                slot: reader.u32()?,
            },
            Op::AcceptTrade => InventoryAction::AcceptTrade(TradeAcceptance {
                partner_id: reader.u32()?,
                trade_stamp: reader.f64()?,
                status: reader.u32()?,
                initiator_id: reader.u32()?,
                initiator_accepts: reader.u32()? != 0,
                partner_accepts: reader.u32()? != 0,
            }),
            Op::DeclineTrade => InventoryAction::DeclineTrade,
            Op::ResetTrade => InventoryAction::ResetTrade,
            Op::NoLongerViewingContents => InventoryAction::StopViewing(reader.u32()?),
            _ => return Err(WireError::UnexpectedOpcode(opcode.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: reader.remaining(),
        })
    }
}
