//! Approved divergence #14: client DispatchUI_UpdateSkillAC @0x006AF280.
//! Source SHA256 c8ec85070effbdcdc55aa718286f3191894e256296ef9aada47cd468f163c632.
//! Object-scoped format does not authorize broadcasting private skill data.
use crate::{WireError, Writer};
pub struct SkillClassUpdate {
    pub sequence: u8,
    pub object: u32,
    pub skill: u32,
    pub advancement: u32,
}
impl SkillClassUpdate {
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        if self.object == 0 || self.advancement > 3 {
            return Err(WireError::InvalidEncoding);
        }
        let mut w = Writer::new();
        w.u32(0x2e2);
        w.bytes(&[self.sequence]);
        w.u32(self.object);
        w.u32(self.skill);
        w.u32(self.advancement);
        Ok(w.into_bytes())
    }
}
