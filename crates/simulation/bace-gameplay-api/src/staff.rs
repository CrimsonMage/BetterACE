//! Staff authority is supplied by authenticated server policy, never wire fields.
use crate::{ActionContext, CharacterBinding};
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StaffPrivileges {
    pub account_access: u8,
    pub advocate: bool,
    pub sentinel: bool,
    pub envoy: bool,
    pub developer: bool,
    pub admin: bool,
    pub psr: bool,
}
impl StaffPrivileges {
    pub fn map_teleport(self) -> bool {
        self.admin || self.developer || self.psr
    }
    pub fn allows(self, minimum: u8, sudo: bool) -> bool {
        if minimum > 5 || self.account_access > 5 {
            return false;
        }
        if sudo {
            return self.account_access >= minimum;
        }
        match minimum {
            0 => true,
            1 => self.advocate || self.sentinel || self.envoy || self.developer || self.admin,
            2 => self.sentinel || self.envoy || self.developer || self.admin,
            3 => self.envoy || self.developer || self.admin,
            4 => self.developer || self.admin,
            5 => self.admin,
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapTeleportRequest {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaffDestination {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedMapTeleport {
    pub requested_cell: u32,
    pub requested_xy: [f32; 2],
    pub destination: StaffDestination,
    pub entirely_water: bool,
    pub expected_epoch: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffError {
    Invalid,
    NotAuthorized,
    NotBound,
    Stale,
    Busy,
    Capacity,
    MissingTarget,
    MissingGeometry,
    Water,
    Overflow,
}
#[derive(Clone, Debug, PartialEq)]
pub enum StaffEvent {
    TargetQuery(crate::selection::TargetQueryEvent),
    GagProposal(crate::staff_gags::StaffGagProposal),
    Broadcast {
        token: u64,
        context: ActionContext,
        sender: String,
        recipients: Vec<CharacterBinding>,
        text: String,
    },
    Scripts {
        context: ActionContext,
        targets: Vec<(EntityId, u32)>,
    },
    SpellProposal(StaffSpellTicket),
    Spellbook {
        context: ActionContext,
        spell: u32,
        name: String,
        learn: bool,
        changed: bool,
        revision: u64,
    },
    Outcome {
        token: u64,
        actor: Option<EntityId>,
        result: Result<(), StaffError>,
    },
    Inspection {
        context: ActionContext,
        target: EntityId,
        lines: Vec<String>,
    },
    Teleported {
        context: ActionContext,
        target: EntityId,
        before: StaffDestination,
        after: StaffDestination,
        epoch: u16,
        velocity: [f32; 3],
        grounded: bool,
    },
    Healed {
        context: ActionContext,
        target: EntityId,
        vitals: Vec<(u8, u32)>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaffRegistration {
    pub binding: CharacterBinding,
    pub privileges: StaffPrivileges,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffInspection {
    Identity,
    WhoAmI,
    SelfPosition,
    Gps,
    Position,
    Enchantments,
    Vitals,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaffCommand {
    pub token: u64,
    pub action: StaffAction,
}
#[derive(Clone, Debug, PartialEq)]
pub enum StaffAction {
    QueryTarget {
        context: ActionContext,
        kind: crate::selection::TargetQueryKind,
        target: EntityId,
        mana: Option<crate::selection::PreparedItemManaQuery>,
    },
    Regenerate {
        context: ActionContext,
        target: EntityId,
        sudo: bool,
    },
    Gag {
        context: ActionContext,
        target: crate::social::SocialIdentity,
        requested_name: String,
        enabled: bool,
        unix_seconds: f64,
        sudo: bool,
    },
    GagCommitted(crate::staff_gags::StaffGagProposal),
    GagRejected(crate::staff_gags::StaffGagProposal),
    Broadcast {
        context: ActionContext,
        text: String,
        emote: bool,
        local: bool,
        sudo: bool,
    },
    /// An already committed staff action's Audit record. The simulation owns
    /// in-world recipients and external feed admission through SocialEvent.
    Audit {
        context: ActionContext,
        texts: Vec<String>,
        sudo: bool,
    },
    GrantExperience {
        context: ActionContext,
        target: EntityId,
        amount: u64,
        sudo: bool,
    },
    Buff {
        context: ActionContext,
        target: EntityId,
        fellowship: bool,
        maximum_level: u8,
        equipment: Vec<StaffBaneItem>,
        sudo: bool,
    },
    Spellbook {
        context: ActionContext,
        spell: u32,
        learn: bool,
        sudo: bool,
    },
    SpellCommitted(StaffSpellTicket),
    SpellRejected(StaffSpellTicket),
    Register(StaffRegistration),
    Refresh(StaffRegistration),
    Remove(CharacterBinding),
    MapTeleport {
        context: ActionContext,
        request: MapTeleportRequest,
        prepared: PreparedMapTeleport,
    },
    Teleport {
        context: ActionContext,
        target: EntityId,
        destination: StaffDestination,
        expected_epoch: u16,
        kind: StaffTeleportKind,
        sudo: bool,
    },
    Inspect {
        context: ActionContext,
        target: EntityId,
        kind: StaffInspection,
        sudo: bool,
    },
    CastSpell {
        context: ActionContext,
        target: Option<EntityId>,
        spell: u32,
        sudo: bool,
    },
    Run {
        context: ActionContext,
        mode: StaffRunMode,
        sudo: bool,
    },
    Heal {
        context: ActionContext,
        target: EntityId,
        /// Copied from the accepted object blueprint at command preparation.
        /// Used only for ACE's selected-nonplayer rejection chat.
        target_name: Option<String>,
        sudo: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffTeleportKind {
    ToPlayer,
    BringPlayer,
    ReturnPlayer,
    Location,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffRunMode {
    On,
    Off,
    Toggle,
    Check,
}

/// Immutable verified DAT/enum projection; registration is trusted cold ingress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffSpellDefinition {
    pub spell: u32,
    pub name: String,
    pub enum_name: String,
    pub targeted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffSpellTicket {
    pub operation: u64,
    pub context: ActionContext,
    pub spell: u32,
    pub name: String,
    pub learn: bool,
    pub before_revision: u64,
    pub after_revision: u64,
    pub before: Vec<u32>,
    pub after: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffBuffPlan {
    pub level: u8,
    pub self_spells: Vec<u32>,
    pub other_spells: Vec<u32>,
    pub banes: Vec<u32>,
    pub effects: Vec<(u32, u32)>,
    pub missing: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaffBaneItem {
    pub item: EntityId,
    pub revision: u64,
    pub template: u32,
    pub eligible: bool,
}
