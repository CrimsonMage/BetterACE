//! Chat-window channel and message types used by ChatMessage and the chat GameMessages.
//! Provenance: official ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b, `Source/ACE.Entity/Enum/ChatMessageType.cs` (C# uint).
//! Members and values are checked against the ACE source by
//! `oracle/spewbox_enums_generate.py` and `tests/spewbox_enums.rs`.
//! Rust names are the ACE names with underscores removed. The ACE spelling is in the CSV.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ChatMessageType {
    Broadcast = 0x0,
    AllChannels = 0x1,
    Speech = 0x2,
    Tell = 0x3,
    OutgoingTell = 0x4,
    System = 0x5,
    Combat = 0x6,
    Magic = 0x7,
    Channel = 0x8,
    ChannelSend = 0x9,
    Social = 0xa,
    SocialSend = 0xb,
    Emote = 0xc,
    Advancement = 0xd,
    Abuse = 0xe,
    Help = 0xf,
    Appraisal = 0x10,
    Spellcasting = 0x11,
    Allegiance = 0x12,
    Fellowship = 0x13,
    WorldBroadcast = 0x14,
    CombatEnemy = 0x15,
    CombatSelf = 0x16,
    Recall = 0x17,
    Craft = 0x18,
    Salvaging = 0x19,
    X1B = 0x1b,
    X1C = 0x1c,
    X1D = 0x1d,
    X1E = 0x1e,
    AdminTell = 0x1f,
}

impl ChatMessageType {
    /// Every member, sorted by wire value. `from_u32` binary-searches this table.
    pub const ALL: [Self; 31] = [
        Self::Broadcast,
        Self::AllChannels,
        Self::Speech,
        Self::Tell,
        Self::OutgoingTell,
        Self::System,
        Self::Combat,
        Self::Magic,
        Self::Channel,
        Self::ChannelSend,
        Self::Social,
        Self::SocialSend,
        Self::Emote,
        Self::Advancement,
        Self::Abuse,
        Self::Help,
        Self::Appraisal,
        Self::Spellcasting,
        Self::Allegiance,
        Self::Fellowship,
        Self::WorldBroadcast,
        Self::CombatEnemy,
        Self::CombatSelf,
        Self::Recall,
        Self::Craft,
        Self::Salvaging,
        Self::X1B,
        Self::X1C,
        Self::X1D,
        Self::X1E,
        Self::AdminTell,
    ];

    /// Wire value. The discriminant is the ACE value.
    pub const fn value(self) -> u32 {
        self as u32
    }

    /// Returns the member for a wire value, or `None` for any value ACE does not define.
    pub fn from_u32(value: u32) -> Option<Self> {
        match Self::ALL.binary_search_by_key(&value, |member| *member as u32) {
            Ok(index) => Some(Self::ALL[index]),
            Err(_) => None,
        }
    }
}
