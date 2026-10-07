//! Official ACE `GameMessageGroup.cs` at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! Identifiers alone do not establish payload implementation.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameMessageGroup(pub u32);

#[allow(non_upper_case_globals)]
impl GameMessageGroup {
    pub const InvalidQueue: Self = Self(0x00);
    pub const EventQueue: Self = Self(0x01);
    pub const ControlQueue: Self = Self(0x02);
    pub const WeenieQueue: Self = Self(0x03);
    pub const LoginQueue: Self = Self(0x04);
    pub const DatabaseQueue: Self = Self(0x05);
    pub const SecureControlQueue: Self = Self(0x06);
    pub const SecureWeenieQueue: Self = Self(0x07);
    pub const SecureLoginQueue: Self = Self(0x08);
    pub const UIQueue: Self = Self(0x09);
    pub const SmartboxQueue: Self = Self(0x0A);
    pub const ObserverQueue: Self = Self(0x0B);
    pub const QueueMax: Self = Self(0x0C);
    /// All upstream names, including aliases sharing a numeric identifier.
    pub const NAMED: &'static [(&'static str, Self)] = &[
        ("InvalidQueue", Self::InvalidQueue),
        ("EventQueue", Self::EventQueue),
        ("ControlQueue", Self::ControlQueue),
        ("WeenieQueue", Self::WeenieQueue),
        ("LoginQueue", Self::LoginQueue),
        ("DatabaseQueue", Self::DatabaseQueue),
        ("SecureControlQueue", Self::SecureControlQueue),
        ("SecureWeenieQueue", Self::SecureWeenieQueue),
        ("SecureLoginQueue", Self::SecureLoginQueue),
        ("UIQueue", Self::UIQueue),
        ("SmartboxQueue", Self::SmartboxQueue),
        ("ObserverQueue", Self::ObserverQueue),
        ("QueueMax", Self::QueueMax),
    ];
}
