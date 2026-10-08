/// ACE PropertyAttribute identifiers, preserving the client ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AttributeId {
    Strength = 1,
    Endurance = 2,
    Quickness = 3,
    Coordination = 4,
    Focus = 5,
    SelfAttribute = 6,
}

/// Only the maximum-vital properties can receive progression XP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VitalId {
    MaxHealth = 1,
    MaxStamina = 3,
    MaxMana = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidProgressionId(pub u32);

impl TryFrom<u32> for AttributeId {
    type Error = InvalidProgressionId;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Strength),
            2 => Ok(Self::Endurance),
            3 => Ok(Self::Quickness),
            4 => Ok(Self::Coordination),
            5 => Ok(Self::Focus),
            6 => Ok(Self::SelfAttribute),
            _ => Err(InvalidProgressionId(value)),
        }
    }
}

impl TryFrom<u32> for VitalId {
    type Error = InvalidProgressionId;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::MaxHealth),
            3 => Ok(Self::MaxStamina),
            5 => Ok(Self::MaxMana),
            _ => Err(InvalidProgressionId(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProgressionTarget {
    Attribute(AttributeId),
    Vital(VitalId),
    /// Must resolve to a skill owned by the character before expenditure.
    Skill(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SkillAdvancement {
    Inactive = 0,
    Untrained = 1,
    Trained = 2,
    Specialized = 3,
}

impl TryFrom<u32> for SkillAdvancement {
    type Error = InvalidProgressionId;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Inactive),
            1 => Ok(Self::Untrained),
            2 => Ok(Self::Trained),
            3 => Ok(Self::Specialized),
            _ => Err(InvalidProgressionId(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaiseProgression {
    pub target: ProgressionTarget,
    pub amount: u32,
}

/// Authoritative inputs required by ACE's private trait-update messages. These
/// are domain data, not packet layouts. Absence must never be filled with zeros.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TraitDetails {
    Attribute {
        starting_value: u32,
    },
    Vital {
        starting_value: u32,
        current: u32,
    },
    Skill {
        initial_level: u32,
        resistance_at_last_check: u32,
        last_used_time: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressionProjection {
    pub target: ProgressionTarget,
    pub experience_spent: u32,
    pub ranks: u16,
    pub advancement: SkillAdvancement,
    pub details: Option<TraitDetails>,
}

/// Read projection produced only after a complete in-memory transition.
/// Persistence adapters must retain its revision until saved; it is not a save DTO.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressionChange {
    pub before: ProgressionProjection,
    pub after: ProgressionProjection,
    pub available_experience: u64,
    pub revision: u64,
    /// Frozen by the simulation after refreshing derived values. The source
    /// rank announcement must use the accepted base, including formulas and
    /// bonuses, rather than infer it from the trait update's rank field.
    pub rank_effect: Option<RankEffect>,
    /// Pinned ACE sends a second full private vital update after the rank
    /// announcement for Endurance. The simulation freezes this from its
    /// accepted character and World owners; a missing value remains explicit.
    pub follow_up_vital: Option<ProgressionProjection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RankEffect {
    pub base: u32,
    pub reached_maximum: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionRejection {
    UnknownTarget,
    UntrainedSkill,
    InsufficientExperience,
    MaximumRank,
    ExceedsMaximumExperience,
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionActionRejection {
    /// An authoritative reservation prevented the attempt before its sequence
    /// was consumed. Only this rejection permits retrying the same intent.
    DurabilityPending,
    NotBound,
    OwnershipMismatch,
    MissingActor,
    StaleSequence,
    Domain(ProgressionRejection),
}

pub type ProgressionOutcome = crate::ActionResult<ProgressionChange, ProgressionActionRejection>;
