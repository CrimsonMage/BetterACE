//! ACE initial title collection; authoritative ownership is supplied by the caller.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::{GameEventType, GameMessageOpcode};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterTitle {
    pub current: u32,
    pub titles: Vec<u32>,
}
impl CharacterTitle {
    pub fn encode(
        &self,
        object_id: u32,
        sequence: u32,
        max_titles: usize,
        max_message_bytes: usize,
    ) -> Result<Vec<u8>, WireError> {
        if self.titles.len() > max_titles || self.titles.len() > i32::MAX as usize {
            return Err(WireError::LimitExceeded);
        }
        let len = self
            .titles
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(28))
            .ok_or(WireError::LimitExceeded)?;
        if len > max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut w = message_writer(GameMessageOpcode::GameEvent);
        for word in [
            object_id,
            sequence,
            GameEventType::CharacterTitle.0,
            1,
            self.current,
            self.titles.len() as u32,
        ] {
            w.u32(word);
        }
        for id in &self.titles {
            w.u32(*id);
        }
        Ok(w.into_bytes())
    }
}
