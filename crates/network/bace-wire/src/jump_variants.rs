//! Client JumpPack::Pack (0x00516D10) contains Position before its epochs;
//! CM_Movement::Event_Jump_NonAutonomous (0x006AFB30) contains only extent.
//! The independently pinned ACE legacy ClientJump decoder remains separate.
use crate::{MovementEpochs, Reader, WireError, WirePosition};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientPositionJump {
    pub extent: f32,
    pub reported_velocity: [f32; 3],
    pub reported_position: WirePosition,
    pub epochs: MovementEpochs,
}
impl ClientPositionJump {
    pub fn decode(bytes: &[u8], limit: usize) -> Result<Self, WireError> {
        if bytes.len() > limit {
            return Err(WireError::LimitExceeded);
        }
        if bytes.len() != 56 {
            return Err(WireError::InvalidLength);
        }
        let mut reader = Reader::new(bytes);
        Ok(Self {
            extent: reader.f32()?,
            reported_velocity: [reader.f32()?, reader.f32()?, reader.f32()?],
            reported_position: WirePosition::decode(&mut reader)?,
            epochs: MovementEpochs::decode(&mut reader)?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientNonAutonomousJump {
    pub extent: f32,
}
impl ClientNonAutonomousJump {
    pub fn decode(bytes: &[u8], limit: usize) -> Result<Self, WireError> {
        if bytes.len() > limit {
            return Err(WireError::LimitExceeded);
        }
        if bytes.len() != 4 {
            return Err(WireError::InvalidLength);
        }
        Ok(Self {
            extent: Reader::new(bytes).f32()?,
        })
    }
}
