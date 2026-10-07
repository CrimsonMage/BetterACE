//! ACE RestrictionDB version and GUID bucket ordering. Permission decisions are
//! supplied by the owning housing service; this codec does not grant access.
use crate::{WireError, Writer};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestrictionPermission {
    pub object_id: u32,
    pub permissions: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectRestrictions {
    pub open: bool,
    pub monarch_id: u32,
    pub permissions: Vec<RestrictionPermission>,
}
impl ObjectRestrictions {
    pub fn encode(&self, max_permissions: usize) -> Result<Vec<u8>, WireError> {
        let mut writer = Writer::new();
        self.write(&mut writer, max_permissions)?;
        Ok(writer.into_bytes())
    }
    pub(crate) fn write(
        &self,
        writer: &mut Writer,
        max_permissions: usize,
    ) -> Result<(), WireError> {
        if self.permissions.len() > max_permissions
            || self.permissions.len() > usize::from(u16::MAX)
        {
            return Err(WireError::LimitExceeded);
        }
        let mut sorted: Vec<_> = self.permissions.iter().collect();
        sorted.sort_unstable_by_key(|entry| (entry.object_id % 89, entry.object_id));
        if sorted
            .windows(2)
            .any(|entries| entries[0].object_id == entries[1].object_id)
        {
            return Err(WireError::InvalidEncoding);
        }
        writer.u32(0x10000002);
        writer.u32(u32::from(self.open));
        writer.u32(self.monarch_id);
        writer.u16(sorted.len() as u16);
        writer.u16(768);
        for entry in sorted {
            writer.u32(entry.object_id);
            writer.u32(entry.permissions);
        }
        Ok(())
    }
}
