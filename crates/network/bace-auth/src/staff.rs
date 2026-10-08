//! Official ACE CommandManager and Player.SetEphemeralValues privilege policy.
//! Account access and character flags are deliberately separate inputs.
use crate::AccessLevel;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterPrivileges {
    pub advocate: bool,
    pub sentinel: bool,
    pub envoy: bool,
    pub developer: bool,
    pub admin: bool,
    pub psr: bool,
}
impl CharacterPrivileges {
    /// Fresh privileges from an account when OverrideCharacterPermissions is on.
    /// Callers must not retain flags belonging to a previously elevated account.
    pub fn from_account(access: AccessLevel) -> Self {
        Self {
            advocate: access == AccessLevel::Advocate,
            sentinel: matches!(access, AccessLevel::Sentinel | AccessLevel::Envoy),
            envoy: access == AccessLevel::Envoy,
            developer: access == AccessLevel::Developer,
            admin: access == AccessLevel::Admin,
            psr: false,
        }
    }
    pub fn allows(self, required: AccessLevel) -> bool {
        match required {
            AccessLevel::Player => true,
            AccessLevel::Advocate => {
                self.advocate || self.sentinel || self.envoy || self.developer || self.admin
            }
            AccessLevel::Sentinel => self.sentinel || self.envoy || self.developer || self.admin,
            AccessLevel::Envoy => self.envoy || self.developer || self.admin,
            AccessLevel::Developer => self.developer || self.admin,
            AccessLevel::Admin => self.admin,
        }
    }
    /// GameActionAdvocateTeleport and client PlayerIsPSR use this exact gate.
    pub fn map_teleport(self) -> bool {
        self.admin || self.developer || self.psr
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaffPrincipal {
    pub account_access: AccessLevel,
    pub character: CharacterPrivileges,
    pub in_world: bool,
}
impl StaffPrincipal {
    pub fn allows(self, minimum: AccessLevel, sudo: bool) -> bool {
        if sudo {
            self.account_access as u8 >= minimum as u8
        } else {
            self.character.allows(minimum)
        }
    }
}
