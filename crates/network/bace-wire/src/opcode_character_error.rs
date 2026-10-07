//! Official ACE `Enum/CharacterError.cs` at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! Identifiers alone do not establish payload implementation.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CharacterError(pub u32);

#[allow(non_upper_case_globals)]
impl CharacterError {
    pub const Logon: Self = Self(0x00000001);
    pub const AccountLogin: Self = Self(0x00000003);
    pub const ServerCrash1: Self = Self(0x00000004);
    pub const Logoff: Self = Self(0x00000005);
    pub const Delete: Self = Self(0x00000006);
    pub const ServerCrash2: Self = Self(0x00000008);
    pub const AccountInvalid: Self = Self(0x00000009);
    pub const AccountDoesntExist: Self = Self(0x0000000A);
    pub const EnterGameGeneric: Self = Self(0x0000000B);
    pub const EnterGameStressAccount: Self = Self(0x0000000C);
    pub const EnterGameCharacterInWorld: Self = Self(0x0000000D);
    pub const EnterGamePlayerAccountMissing: Self = Self(0x0000000E);
    pub const EnterGameCharacterNotOwned: Self = Self(0x0000000F);
    pub const EnterGameCharacterInWorldServer: Self = Self(0x00000010);
    pub const EnterGameOldCharacter: Self = Self(0x00000011);
    pub const EnterGameCorruptCharacter: Self = Self(0x00000012);
    pub const EnterGameStartServerDown: Self = Self(0x00000013);
    pub const EnterGameCouldntPlaceCharacter: Self = Self(0x00000014);
    pub const LogonServerFull: Self = Self(0x00000015);
    pub const EnterGameCharacterLocked: Self = Self(0x00000017);
    pub const SubscriptionExpired: Self = Self(0x00000018);
    /// All upstream names, including aliases sharing a numeric identifier.
    pub const NAMED: &'static [(&'static str, Self)] = &[
        ("Logon", Self::Logon),
        ("AccountLogin", Self::AccountLogin),
        ("ServerCrash1", Self::ServerCrash1),
        ("Logoff", Self::Logoff),
        ("Delete", Self::Delete),
        ("ServerCrash2", Self::ServerCrash2),
        ("AccountInvalid", Self::AccountInvalid),
        ("AccountDoesntExist", Self::AccountDoesntExist),
        ("EnterGameGeneric", Self::EnterGameGeneric),
        ("EnterGameStressAccount", Self::EnterGameStressAccount),
        ("EnterGameCharacterInWorld", Self::EnterGameCharacterInWorld),
        (
            "EnterGamePlayerAccountMissing",
            Self::EnterGamePlayerAccountMissing,
        ),
        (
            "EnterGameCharacterNotOwned",
            Self::EnterGameCharacterNotOwned,
        ),
        (
            "EnterGameCharacterInWorldServer",
            Self::EnterGameCharacterInWorldServer,
        ),
        ("EnterGameOldCharacter", Self::EnterGameOldCharacter),
        ("EnterGameCorruptCharacter", Self::EnterGameCorruptCharacter),
        ("EnterGameStartServerDown", Self::EnterGameStartServerDown),
        (
            "EnterGameCouldntPlaceCharacter",
            Self::EnterGameCouldntPlaceCharacter,
        ),
        ("LogonServerFull", Self::LogonServerFull),
        ("EnterGameCharacterLocked", Self::EnterGameCharacterLocked),
        ("SubscriptionExpired", Self::SubscriptionExpired),
    ];
}
