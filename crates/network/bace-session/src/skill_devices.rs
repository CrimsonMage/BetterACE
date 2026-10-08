//! Authenticated Use routing selected by prepared object kind, plus official
//! ConfirmationType AlterSkill (2), AlterAttribute (3), and Augmentation (6).
use crate::{DispatchError, SessionState};
use bace_gameplay_api::ActionContext;
use bace_wire::{
    CraftingAction, CraftingRequest, GameActionEnvelope, InventoryAction, InventoryRequest,
    WireError, opcode::GameActionType,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillDeviceConfirmationType {
    AlterSkill,
    AlterAttribute,
    Augmentation,
}
impl SkillDeviceConfirmationType {
    pub fn wire_id(self) -> u32 {
        match self {
            Self::AlterSkill => 2,
            Self::AlterAttribute => 3,
            Self::Augmentation => 6,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillDeviceAction {
    Use {
        item: u32,
    },
    Confirmation {
        kind: SkillDeviceConfirmationType,
        token: u32,
        accepted: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchedSkillDevice {
    pub context: ActionContext,
    pub request: SkillDeviceAction,
    pub ignored_trailing_bytes: usize,
}
pub fn decode_skill_device(
    state: SessionState,
    mut binding: ActionContext,
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<DispatchedSkillDevice, DispatchError> {
    if state != SessionState::WorldConnected {
        return Err(DispatchError::WrongState);
    }
    let envelope =
        GameActionEnvelope::decode(bytes, max_payload_bytes).map_err(DispatchError::Wire)?;
    let (request, ignored_trailing_bytes) = match envelope.action {
        GameActionType::Use => {
            let parsed =
                InventoryRequest::decode(envelope.action, envelope.payload, max_payload_bytes, 0)
                    .map_err(DispatchError::Wire)?;
            let InventoryAction::Use(item) = parsed.action else {
                return Err(DispatchError::Wire(WireError::InvalidEncoding));
            };
            (SkillDeviceAction::Use { item }, parsed.trailing_bytes)
        }
        GameActionType::ConfirmationResponse => {
            let parsed =
                CraftingRequest::decode(envelope.action, envelope.payload, max_payload_bytes)
                    .map_err(DispatchError::Wire)?;
            let CraftingAction::Confirmation {
                confirmation_type,
                context,
                accepted,
            } = parsed.action
            else {
                return Err(DispatchError::Wire(WireError::InvalidEncoding));
            };
            let kind = match confirmation_type {
                2 => SkillDeviceConfirmationType::AlterSkill,
                3 => SkillDeviceConfirmationType::AlterAttribute,
                6 => SkillDeviceConfirmationType::Augmentation,
                other => return Err(DispatchError::InvalidTarget(other)),
            };
            (
                SkillDeviceAction::Confirmation {
                    kind,
                    token: context,
                    accepted,
                },
                parsed.trailing_bytes,
            )
        }
        other => return Err(DispatchError::UnsupportedAction(other.0)),
    };
    binding.sequence = envelope.sequence;
    Ok(DispatchedSkillDevice {
        context: binding,
        request,
        ignored_trailing_bytes,
    })
}
