//! Frozen NPC stage effects V1; explicit runtime converters own live types.
use crate::npc_values_v1::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcPropertyChangeV1 {
    pub family: NpcPropertyFamilyV1,
    pub stat: u32,
    pub before: Option<NpcValueV1>,
    pub after: Option<NpcValueV1>,
    pub before_revision: u64,
    pub after_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcQuestProgressV1 {
    pub completions: i32,
    pub last_completed: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcQuestChangeV1 {
    pub name: String,
    pub before: Option<NpcQuestProgressV1>,
    pub after: Option<NpcQuestProgressV1>,
    pub before_revision: u64,
    pub after_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcExperienceCreditV1 {
    pub before_revision: u64,
    pub after_revision: u64,
    pub before_available: u64,
    pub after_available: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcLuminanceCreditV1 {
    pub before_revision: u64,
    pub after_revision: u64,
    pub before_available: i64,
    pub before_maximum: i64,
    pub after_available: i64,
    pub after_maximum: i64,
    pub requested: i64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcEffectV1 {
    Property {
        actor: u32,
        aggregate: Option<NpcAggregateFenceV1>,
        change: NpcPropertyChangeV1,
    },
    Quest {
        actor: u32,
        aggregate: Option<NpcAggregateFenceV1>,
        change: NpcQuestChangeV1,
    },
    FellowQuest {
        fellowship: u64,
        change: NpcQuestChangeV1,
    },
    Experience {
        actor: u32,
        credit: NpcExperienceCreditV1,
    },
    Luminance {
        actor: u32,
        credit: NpcLuminanceCreditV1,
    },
    Service(NpcOperationV1),
    CharacterService {
        actor: u32,
        change: NpcCharacterChangeV1,
    },
    Contract {
        actor: u32,
        before_revision: u64,
        change: NpcContractChangeV1,
    },
    EarnedExperience {
        actor: u32,
        change: NpcEarnedExperienceV1,
    },
    // Append-only V1 discriminant: existing variants above are unchanged.
    QueuedExperience {
        actor: u32,
        amount: u64,
        phase: NpcQueuedExperiencePhaseV1,
    },
    QueuedSharedExperience {
        actor: u32,
        amount: u64,
        phase: NpcQueuedExperiencePhaseV1,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcPendingEffectV1 {
    pub ticket: u64,
    pub context: NpcContextV1,
    pub effect: NpcEffectV1,
    pub completion: NpcCompletionV1,
    pub adopted: bool,
    pub detached: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcCharacterStateV1 {
    pub level: u32,
    pub total_experience: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub titles: Vec<u32>,
    pub enlightenment: u32,
    pub sanctuary: Option<NpcDestinationV1>,
    pub total_skill_credits: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcCharacterChangeV1 {
    pub before_revision: u64,
    pub after_revision: u64,
    pub before: NpcCharacterStateV1,
    pub after: NpcCharacterStateV1,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcContractChangeV1 {
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub before: Vec<crate::ContractSaveV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub after: Vec<crate::ContractSaveV1>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcEarnedExperienceV1 {
    pub experience: NpcExperienceCreditV1,
    pub services: NpcCharacterChangeV1,
    pub before_skill_credits: Option<u32>,
    pub after_skill_credits: Option<u32>,
    pub earned_skill_credits: u32,
    pub credited: u64,
}
fn revision(before: u64, after: u64, changed: bool) -> bool {
    before.checked_add(u64::from(changed)) == Some(after)
}
fn valid_value(value: &NpcValueV1) -> bool {
    match value {
        NpcValueV1::Float(v) => v.is_finite(),
        NpcValueV1::String(v) => v.len() <= 65536,
        _ => true,
    }
}
fn family_value(f: &NpcPropertyFamilyV1, v: &NpcValueV1) -> bool {
    use NpcPropertyFamilyV1 as F;
    use NpcValueV1 as V;
    valid_value(v)
        && matches!(
            (f, v),
            (F::Bool, V::Bool(_))
                | (F::Int, V::Int(_))
                | (F::Int64, V::Int64(_))
                | (F::Float, V::Float(_))
                | (F::String, V::String(_))
                | (
                    F::Attribute
                        | F::RawAttribute
                        | F::Vital
                        | F::RawVital
                        | F::Skill
                        | F::RawSkill
                        | F::SkillAdvancement,
                    V::Unsigned(_)
                )
        )
}
fn character(s: &NpcCharacterStateV1) -> bool {
    s.level > 0
        && s.level <= 10000
        && s.total_experience <= i64::MAX as u64
        && s.titles.len() <= 4096
        && !s.titles.contains(&0)
        && !s.titles.windows(2).any(|v| v[0] >= v[1])
        && s.sanctuary.as_ref().is_none_or(|p| {
            !p.relative
                && p.cell.is_some_and(|c| c != 0)
                && p.position.iter().chain(&p.rotation).all(|v| v.is_finite())
        })
}
fn character_change(c: &NpcCharacterChangeV1) -> bool {
    character(&c.before)
        && character(&c.after)
        && revision(c.before_revision, c.after_revision, c.before != c.after)
}
fn contracts(s: &[crate::ContractSaveV1]) -> bool {
    s.len() <= 100
        && !s.iter().any(|s| s.id == 0)
        && !s.windows(2).any(|v| v[0].id >= v[1].id)
        && s.iter().filter(|s| s.display).count() <= 1
}
fn experience(c: &NpcExperienceCreditV1) -> bool {
    c.before_available <= i64::MAX as u64
        && c.after_available <= i64::MAX as u64
        && revision(
            c.before_revision,
            c.after_revision,
            c.before_available != c.after_available,
        )
}
fn quest(c: &NpcQuestChangeV1) -> bool {
    !c.name.is_empty()
        && c.name.len() <= 256
        && revision(c.before_revision, c.after_revision, c.before != c.after)
}
/// Checked frozen-schema validation only; content/rules and participant CAS are
/// checked separately by the owning runtime operation and database journal.
pub fn validate_npc_effect(value: &NpcEffectV1) -> Result<(), crate::SaveCodecError> {
    let valid = match value {
        NpcEffectV1::Property {
            actor,
            aggregate,
            change,
        } => {
            *actor != 0
                && aggregate.is_none_or(|f| {
                    revision(
                        f.before_revision,
                        f.after_revision,
                        change.before != change.after,
                    )
                })
                && change.stat <= 65535
                && change
                    .before
                    .as_ref()
                    .is_none_or(|v| family_value(&change.family, v))
                && change
                    .after
                    .as_ref()
                    .is_none_or(|v| family_value(&change.family, v))
                && revision(
                    change.before_revision,
                    change.after_revision,
                    change.before != change.after,
                )
        }
        NpcEffectV1::Quest {
            actor,
            aggregate,
            change,
        } => {
            *actor != 0
                && quest(change)
                && aggregate.is_none_or(|f| {
                    revision(
                        f.before_revision,
                        f.after_revision,
                        change.before != change.after,
                    )
                })
        }
        NpcEffectV1::FellowQuest { fellowship, change } => *fellowship != 0 && quest(change),
        NpcEffectV1::Experience { actor, credit } => *actor != 0 && experience(credit),
        NpcEffectV1::Luminance { actor, credit: c } => {
            *actor != 0
                && c.before_available >= 0
                && c.before_maximum >= c.before_available
                && c.after_available >= 0
                && c.after_maximum >= c.after_available
                && revision(
                    c.before_revision,
                    c.after_revision,
                    c.before_available != c.after_available || c.before_maximum != c.after_maximum,
                )
        }
        NpcEffectV1::CharacterService { actor, change } => *actor != 0 && character_change(change),
        NpcEffectV1::Contract {
            actor,
            before_revision,
            change,
        } => {
            *actor != 0
                && contracts(&change.before)
                && contracts(&change.after)
                && (change.before == change.after || *before_revision < u64::MAX)
        }
        NpcEffectV1::EarnedExperience { actor, change: c } => {
            *actor != 0
                && character(&c.services.before)
                && character(&c.services.after)
                && c.experience.before_revision == c.services.before_revision
                && c.experience.after_revision == c.services.after_revision
                && c.experience.before_available.checked_add(c.credited)
                    == Some(c.experience.after_available)
                && c.services.before.total_experience.checked_add(c.credited)
                    == Some(c.services.after.total_experience)
                && c.services.after.level >= c.services.before.level
                && i32::try_from(c.earned_skill_credits).is_ok_and(|earned| {
                    c.services
                        .before
                        .total_skill_credits
                        .map(|v| v.checked_add(earned))
                        == c.services.after.total_skill_credits.map(Some)
                })
                && c.before_skill_credits
                    .map(|v| v.checked_add(c.earned_skill_credits))
                    == c.after_skill_credits.map(Some)
                && (c.before_skill_credits.is_some() || c.earned_skill_credits == 0)
                && c.experience.after_available <= i64::MAX as u64
                && c.after_skill_credits.is_none_or(|v| v <= i32::MAX as u32)
                && revision(
                    c.services.before_revision,
                    c.services.after_revision,
                    c.services.before != c.services.after
                        || c.experience.before_available != c.experience.after_available
                        || c.before_skill_credits != c.after_skill_credits,
                )
        }
        NpcEffectV1::QueuedExperience { actor, amount, .. }
        | NpcEffectV1::QueuedSharedExperience { actor, amount, .. } => {
            *actor != 0 && *amount <= i64::MAX as u64
        }
        NpcEffectV1::Service(op) => crate::npc_workflow_validation::operation(op),
    };
    if valid {
        Ok(())
    } else {
        Err(crate::SaveCodecError::Invalid("NPC frozen effect"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcAggregateFenceV1 {
    pub before_revision: u64,
    pub after_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NpcQueuedExperiencePhaseV1 {
    AwaitingAdmission,
    Ready,
}
pub fn validate_npc_pending_effect(
    value: &NpcPendingEffectV1,
) -> Result<(), crate::SaveCodecError> {
    validate_npc_effect(&value.effect)?;
    if let NpcEffectV1::QueuedExperience { phase, .. }
    | NpcEffectV1::QueuedSharedExperience { phase, .. } = value.effect
        && (value.adopted || value.detached != (phase == NpcQueuedExperiencePhaseV1::Ready))
    {
        return Err(crate::SaveCodecError::Invalid("queued XP phase"));
    }
    Ok(())
}
