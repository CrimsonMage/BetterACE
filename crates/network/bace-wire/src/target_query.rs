//! Pinned QueryHealth/QueryItemMana readers and QueryItemManaResponse writer.
use crate::{
    Reader, WireError, Writer,
    opcode::{GameActionType, GameEventType, GameMessageOpcode},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetQueryInput {
    Health(u32),
    ItemMana(u32),
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
