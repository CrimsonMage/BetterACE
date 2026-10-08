//! Official ACE 47edade3 crafting inputs and event bodies. Result cardinality
//! intentionally follows supplied retail observations #30–32.
use crate::opcode::{GameActionType as Action, GameEventType as Event};
use crate::{GameEventEnvelope, InventoryAction, InventoryRequest, Reader, WireError, Writer};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CraftingAction {
    UseWithTarget {
        source_id: u32,
        target_id: u32,
    },
    Salvage {
        tool_id: u32,
        items: Vec<u32>,
    },
    Confirmation {
        confirmation_type: u32,
        context: u32,
        accepted: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CraftingRequest {
    pub action: CraftingAction,
    pub trailing_bytes: usize,
}
impl CraftingRequest {
    pub fn decode(
        opcode: Action,
        payload: &[u8],
        max_payload_bytes: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        let action = match opcode {
            Action::UseWithTarget => {
                let parsed = InventoryRequest::decode(opcode, payload, max_payload_bytes, 0)?;
                let InventoryAction::UseWithTarget {
                    source_id,
                    target_id,
                } = parsed.action
                else {
                    return Err(WireError::InvalidEncoding);
                };
                return Ok(Self {
                    action: CraftingAction::UseWithTarget {
                        source_id,
                        target_id,
                    },
                    trailing_bytes: parsed.trailing_bytes,
                });
            }
            Action::CreateTinkeringTool => {
                let tool_id = r.u32()?;
                let count = r.u32()? as usize;
                if count > 300 {
                    return Err(WireError::LimitExceeded);
                }
                if count > r.remaining() / 4 {
                    return Err(WireError::Truncated);
                }
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(r.u32()?);
                }
                CraftingAction::Salvage { tool_id, items }
            }
            Action::ConfirmationResponse => CraftingAction::Confirmation {
                confirmation_type: r.u32()?,
                context: r.u32()?,
                accepted: r.u32()? != 0,
            },
            other => return Err(WireError::UnexpectedOpcode(other.0)),
        };
        Ok(Self {
            action,
            trailing_bytes: r.remaining(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SalvageWireResult {
    pub material: u32,
    pub workmanship: f64,
    pub units: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CraftingEvent<'a> {
    ConfirmationRequest {
        confirmation_type: u32,
        context: u32,
        text: &'a str,
    },
    ConfirmationDone {
        confirmation_type: u32,
        context: u32,
    },
    SalvageResult {
        skill: u32,
        unsuitable: &'a [u32],
        result: Option<SalvageWireResult>,
        augmentation_bonus: u32,
    },
}
impl CraftingEvent<'_> {
    pub fn encode(
        &self,
        actor: u32,
        sequence: u32,
        max_bytes: usize,
    ) -> Result<Vec<u8>, WireError> {
        let mut w = Writer::new();
        let event = match self {
            Self::ConfirmationRequest {
                confirmation_type,
                context,
                text,
            } => {
                if text.len() > 4096 {
                    return Err(WireError::LimitExceeded);
                }
                w.u32(*confirmation_type);
                w.u32(*context);
                w.string16(text)?;
                Event::CharacterConfirmationRequest
            }
            Self::ConfirmationDone {
                confirmation_type,
                context,
            } => {
                w.u32(*confirmation_type);
                w.u32(*context);
                Event::CharacterConfirmationDone
            }
            Self::SalvageResult {
                skill,
                unsuitable,
                result,
                augmentation_bonus,
            } => {
                if unsuitable.len() > 300 {
                    return Err(WireError::LimitExceeded);
                }
                if *augmentation_bonus > 100 || *augmentation_bonus % 25 != 0 {
                    return Err(WireError::InvalidEncoding);
                }
                w.u32(*skill);
                w.u32(unsuitable.len() as u32);
                for id in *unsuitable {
                    w.u32(*id);
                }
                w.u32(u32::from(result.is_some()));
                if let Some(result) = result {
                    if !result.workmanship.is_finite()
                        || result.workmanship <= 0.0
                        || result.units == 0
                        || result.material == 0
                    {
                        return Err(WireError::InvalidEncoding);
                    }
                    w.u32(result.material);
                    w.f64(result.workmanship);
                    w.u32(result.units);
                }
                w.u32(*augmentation_bonus);
                Event::SalvageOperationsResult
            }
        };
        let payload = w.into_bytes();
        if payload
            .len()
            .checked_add(16)
            .is_none_or(|size| size > max_bytes)
        {
            return Err(WireError::LimitExceeded);
        }
        GameEventEnvelope {
            object_id: actor,
            sequence,
            event,
            payload: &payload,
        }
        .encode(max_bytes.saturating_sub(16))
    }
}
