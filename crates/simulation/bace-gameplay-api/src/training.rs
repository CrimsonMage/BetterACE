use crate::{ActionResult, ProgressionProjection};

/// Client quoted skill credits remain untrusted and signed, matching the action
/// payload. The domain compares them with prepared DAT prices before spending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainSkill {
    pub skill: u32,
    pub quoted_credits: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillTrainingChange {
    pub before: ProgressionProjection,
    pub after: ProgressionProjection,
    pub available_skill_credits: u32,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillTrainingRejection {
    Unavailable,
    UnknownSkill,
    NegativeQuotedCost,
    PriceMismatch,
    InsufficientCredits,
    AlreadyTrained,
    NotTrained,
    MissingTraitDetails,
    UnsupportedAugmentation,
    TraitCapacity,
    RevisionExhausted,
    ExperienceBeyondMaximum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillTrainingActionRejection {
    NotBound,
    OwnershipMismatch,
    MissingActor,
    StaleSequence,
    Domain(SkillTrainingRejection),
}

pub type SkillTrainingOutcome = ActionResult<SkillTrainingChange, SkillTrainingActionRejection>;
