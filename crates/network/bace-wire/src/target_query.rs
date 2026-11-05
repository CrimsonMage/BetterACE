//! Pinned QueryHealth/QueryItemMana/QueryAge/QueryBirth readers and QueryItemManaResponse writer.
use crate::{
    Reader, WireError, Writer,
    opcode::{GameActionType, GameEventType, GameMessageOpcode},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetQueryInput {
    Health(u32),
    ItemMana(u32),
    /// Retail `CM_Character::Event_QueryAge` (0x1C2) payload: one `u32` target.
    Age(u32),
    /// Retail `CM_Character::Event_QueryBirth` (0x1C4) payload: one `u32` target.
    Birth(u32),
}
impl TargetQueryInput {
    pub fn decode(
        action: GameActionType,
        payload: &[u8],
        maximum: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > maximum {
            return Err(WireError::LimitExceeded);
        }
        let mut r = Reader::new(payload);
        match action {
            GameActionType::QueryHealth => Ok(Self::Health(r.u32()?)),
            GameActionType::QueryItemMana => Ok(Self::ItemMana(r.u32()?)),
            // Retail writes exactly one u32 and no trailing bytes (4-byte aligned payload).
            GameActionType::QueryAge | GameActionType::QueryBirth if payload.len() != 4 => {
                Err(if payload.len() < 4 {
                    WireError::Truncated
                } else {
                    WireError::InvalidLength
                })
            }
            GameActionType::QueryAge => Ok(Self::Age(r.u32()?)),
            GameActionType::QueryBirth => Ok(Self::Birth(r.u32()?)),
            _ => Err(WireError::UnexpectedOpcode(action.0)),
        }
    }
}
pub fn encode_item_mana_query(
    actor: u32,
    sequence: u32,
    target: u32,
    fraction: f32,
    success: u32,
    maximum: usize,
) -> Result<Vec<u8>, WireError> {
    if success > 1 {
        return Err(WireError::InvalidLength);
    }
    let mut w = Writer::new();
    for value in [
        GameMessageOpcode::GameEvent.0,
        actor,
        sequence,
        GameEventType::QueryItemManaResponse.0,
        target,
    ] {
        w.u32(value);
    }
    w.f32(fraction);
    w.u32(success);
    let bytes = w.into_bytes();
    if bytes.len() > maximum {
        return Err(WireError::LimitExceeded);
    }
    Ok(bytes)
}
