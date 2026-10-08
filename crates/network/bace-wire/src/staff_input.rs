//! Official GameActionAdvocateTeleport: ignored target string, then Position.
use crate::{Reader, WireError, WirePosition};
#[derive(Clone, Debug, PartialEq)]
pub struct MapTeleportInput {
    pub ignored_target: String,
    pub position: WirePosition,
    pub trailing_bytes: usize,
}
impl MapTeleportInput {
    pub fn decode(
        payload: &[u8],
        max_payload: usize,
        max_string: usize,
    ) -> Result<Self, WireError> {
        if payload.len() > max_payload {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(payload);
        let ignored_target = reader.client_string16(max_string)?;
        let position = WirePosition::decode(&mut reader)?;
        Ok(Self {
            ignored_target,
            position,
            trailing_bytes: reader.remaining(),
        })
    }
}
