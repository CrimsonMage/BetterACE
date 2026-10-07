//! Bounded projected friend/squelch tables. The wire codec performs no account
//! lookup and cannot reveal account names: ACE transmits the account table empty.
use crate::{WireError, Writer};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FriendsUpdateKind {
    Full = 0,
    Added = 1,
    Removed = 2,
    StatusChanged = 4,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendEntry {
    pub object_id: u32,
    pub online: bool,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendsUpdate {
    pub kind: FriendsUpdateKind,
    pub friends: Vec<FriendEntry>,
}
impl FriendsUpdate {
    pub(crate) fn write(
        &self,
        writer: &mut Writer,
        max_entries: usize,
        max_string_bytes: usize,
    ) -> Result<(), WireError> {
        if self.friends.len() > max_entries || self.friends.len() > u32::MAX as usize {
            return Err(WireError::LimitExceeded);
        }
        if self.kind != FriendsUpdateKind::Full && self.friends.len() != 1 {
            return Err(WireError::InvalidEncoding);
        }
        writer.u32(self.friends.len() as u32);
        for friend in &self.friends {
            writer.u32(friend.object_id);
            writer.u32(u32::from(friend.online));
            writer.u32(0);
            string(writer, &friend.name, max_string_bytes)?;
            writer.u32(0);
            writer.u32(0);
        }
        writer.u32(self.kind as u32);
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquelchInfo {
    pub filters: Vec<u32>,
    pub player_name: String,
    pub account: bool,
}
impl SquelchInfo {
    fn write(
        &self,
        writer: &mut Writer,
        max_filters: usize,
        max_string_bytes: usize,
    ) -> Result<(), WireError> {
        if self.filters.len() > max_filters || self.filters.len() > i32::MAX as usize {
            return Err(WireError::LimitExceeded);
        }
        writer.u32(self.filters.len() as u32);
        for filter in &self.filters {
            writer.u32(*filter);
        }
        string(writer, &self.player_name, max_string_bytes)?;
        writer.u32(u32::from(self.account));
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquelchEntry {
    pub object_id: u32,
    pub info: SquelchInfo,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquelchDatabase {
    pub characters: Vec<SquelchEntry>,
    pub global: SquelchInfo,
}
impl SquelchDatabase {
    pub(crate) fn write(
        &self,
        writer: &mut Writer,
        max_entries: usize,
        max_filters: usize,
        max_string_bytes: usize,
    ) -> Result<(), WireError> {
        if self.characters.len() > max_entries || self.characters.len() > usize::from(u16::MAX) {
            return Err(WireError::LimitExceeded);
        }
        let mut sorted: Vec<_> = self.characters.iter().collect();
        sorted.sort_unstable_by_key(|entry| (entry.object_id % 32, entry.object_id));
        if sorted
            .windows(2)
            .any(|entries| entries[0].object_id == entries[1].object_id)
        {
            return Err(WireError::InvalidEncoding);
        }
        writer.u16(0);
        writer.u16(0); // ACE deliberately transmits no account-name table.
        writer.u16(sorted.len() as u16);
        writer.u16(32);
        for entry in sorted {
            writer.u32(entry.object_id);
            entry.info.write(writer, max_filters, max_string_bytes)?;
        }
        self.global.write(writer, max_filters, max_string_bytes)
    }
}
pub(crate) fn string(writer: &mut Writer, value: &str, max_bytes: usize) -> Result<(), WireError> {
    if value.chars().count() > max_bytes {
        return Err(WireError::LimitExceeded);
    }
    writer.string16(value)
}
