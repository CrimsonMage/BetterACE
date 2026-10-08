//! Retained physical contact phases. A receipt acknowledges completed source
//! proc attempts, not necessarily successful spells; the contact owner applies HP.
use crate::weapon_combat::PhysicalKind;
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysicalHitKey {
    pub attacker: EntityId,
    pub operation: u64,
    pub ordinal: u32,
    pub target: EntityId,
    pub kind: PhysicalKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalProcPhase {
    Attack,
    Dirty,
    Cloak,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PhysicalProcRequest {
    Attack {
        key: PhysicalHitKey,
        weapon: Option<(EntityId, u32)>,
        sigil_target: Option<EntityId>,
        sigil_rolls: [f64; 3],
    },
    Dirty {
        key: PhysicalHitKey,
        weapon: Option<EntityId>,
        spells: [u32; 2],
        count: usize,
    },
    Cloak {
        key: PhysicalHitKey,
        damage: u32,
        roll: f64,
    },
}
impl PhysicalProcRequest {
    pub fn key(&self) -> PhysicalHitKey {
        match self {
            Self::Attack { key, .. } | Self::Dirty { key, .. } | Self::Cloak { key, .. } => *key,
        }
    }
    pub fn phase(&self) -> PhysicalProcPhase {
        match self {
            Self::Attack { .. } => PhysicalProcPhase::Attack,
            Self::Dirty { .. } => PhysicalProcPhase::Dirty,
            Self::Cloak { .. } => PhysicalProcPhase::Cloak,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalProcReceipt {
    pub key: PhysicalHitKey,
    pub phase: PhysicalProcPhase,
    pub damage: Option<u32>,
}
