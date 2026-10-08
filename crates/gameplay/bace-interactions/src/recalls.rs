//! ACE Player_Location command recalls. Durability and authoritative motion are
//! supplied by the simulation owner; this module never accepts a client pose.
use crate::PortalPosition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallKind {
    Lifestone,
    House,
    Marketplace,
    AllegianceHometown,
    AllegianceHousing,
    PkArena,
    PklArena,
}
impl RecallKind {
    pub fn opcode(self) -> u32 {
        match self {
            Self::Lifestone => 0x63,
            Self::House => 0x262,
            Self::Marketplace => 0x28d,
            Self::AllegianceHometown => 0x2ab,
            Self::AllegianceHousing => 0x278,
            Self::PkArena => 0x27,
            Self::PklArena => 0x26,
        }
    }
    pub fn motion(self) -> u32 {
        match self {
            Self::Lifestone => 0x10000153,
            Self::House | Self::AllegianceHousing => 0x1000013a,
            Self::Marketplace => 0x10000166,
            Self::AllegianceHometown => 0x10000171,
            Self::PkArena | Self::PklArena => 0x10000172,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallPolicy {
    pub enabled: [bool; 7],
    pub spell_recalls: bool,
    pub lifestone_binding: bool,
    pub allegiance_binding: bool,
    pub pk_timer_seconds: u32,
}
impl Default for RecallPolicy {
    fn default() -> Self {
        Self {
            enabled: [true; 7],
            spell_recalls: true,
            lifestone_binding: true,
            allegiance_binding: true,
            pk_timer_seconds: 20,
        }
    }
}
impl RecallPolicy {
    pub fn enabled(&self, kind: RecallKind) -> bool {
        self.enabled[match kind {
            RecallKind::Lifestone => 0,
            RecallKind::House => 1,
            RecallKind::Marketplace => 2,
            RecallKind::AllegianceHometown => 3,
            RecallKind::AllegianceHousing => 4,
            RecallKind::PkArena => 5,
            RecallKind::PklArena => 6,
        }]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallError {
    Disabled,
    Olthoi,
    PkOnly,
    PklOnly,
    PkRecent,
    TrainingAcademy,
    Busy,
    NoSanctuary,
    NoHouse,
    NoAllegiance,
    NoHometown,
    NoMansion,
    WrongHouseType,
    MansionClosed,
    MovedTooFar,
    MissingAssets,
    Invalid,
    Capacity,
    Stale,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecallAccess {
    pub olthoi: bool,
    pub pk_status: u32,
    pub pk_recent: bool,
    pub recalls_disabled: bool,
    pub busy: bool,
    pub suicide_in_progress: bool,
}
/// Preserve ACE rejection ordering, including PK status before busy checks.
pub fn check_recall(
    policy: &RecallPolicy,
    kind: RecallKind,
    access: RecallAccess,
) -> Result<(), RecallError> {
    if !policy.enabled(kind) {
        return Err(RecallError::Disabled);
    }
    if access.olthoi && !matches!(kind, RecallKind::Lifestone | RecallKind::PkArena) {
        return Err(RecallError::Olthoi);
    }
    if kind == RecallKind::PkArena && access.pk_status != 4 {
        return Err(RecallError::PkOnly);
    }
    if kind == RecallKind::PklArena && access.pk_status != 64 {
        return Err(RecallError::PklOnly);
    }
    if access.pk_recent {
        return Err(RecallError::PkRecent);
    }
    if access.recalls_disabled {
        return Err(RecallError::TrainingAcademy);
    }
    if access.busy || access.suicide_in_progress {
        return Err(RecallError::Busy);
    }
    Ok(())
}
pub fn recall_delay_ticks(kind: RecallKind, animation_seconds: f64) -> Result<u64, RecallError> {
    let seconds = if kind == RecallKind::Marketplace {
        14.0
    } else {
        animation_seconds
    };
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 300.0 {
        return Err(RecallError::MissingAssets);
    }
    Ok((seconds * 30.0).ceil() as u64)
}
/// ACE Position.SquaredDistanceTo uses f32 landblock offsets even for interiors.
pub fn recall_moved_too_far(
    start: PortalPosition,
    end: PortalPosition,
) -> Result<bool, RecallError> {
    start.validate().map_err(|_| RecallError::Invalid)?;
    end.validate().map_err(|_| RecallError::Invalid)?;
    let a = start.cell >> 16;
    let b = end.cell >> 16;
    let dx = ((a >> 8) as i32 - (b >> 8) as i32) as f32 * 192. + start.origin[0] - end.origin[0];
    let dy = ((a & 255) as i32 - (b & 255) as i32) as f32 * 192. + start.origin[1] - end.origin[1];
    let dz = start.origin[2] - end.origin[2];
    Ok(dx * dx + dy * dy + dz * dz > 64.)
}
