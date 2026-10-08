//! Explicit schema V1 conversion; no evolving owner aggregate is serialized.
use super::values::*;
use bace_simulation::{NpcEffect, NpcPendingCheckpoint, NpcProposal};
use bace_storage_codec::{SaveCodecError, npc_values_v1 as value, npc_workflow_v1 as frozen};
use bace_types::EntityId;
pub(super) fn freeze_property_family(v: bace_entity::PropertyFamily) -> value::NpcPropertyFamilyV1 {
    use bace_entity::PropertyFamily as L;
    use value::NpcPropertyFamilyV1 as F;
    match v {
        L::Bool => F::Bool,
        L::Int => F::Int,
        L::Int64 => F::Int64,
        L::Float => F::Float,
        L::String => F::String,
        L::Attribute => F::Attribute,
        L::RawAttribute => F::RawAttribute,
        L::Vital => F::Vital,
        L::RawVital => F::RawVital,
        L::Skill => F::Skill,
        L::RawSkill => F::RawSkill,
        L::SkillAdvancement => F::SkillAdvancement,
    }
}
pub(super) fn thaw_property_family(v: value::NpcPropertyFamilyV1) -> bace_entity::PropertyFamily {
    use bace_entity::PropertyFamily as L;
    use value::NpcPropertyFamilyV1 as F;
    match v {
        F::Bool => L::Bool,
        F::Int => L::Int,
        F::Int64 => L::Int64,
        F::Float => L::Float,
        F::String => L::String,
        F::Attribute => L::Attribute,
        F::RawAttribute => L::RawAttribute,
        F::Vital => L::Vital,
        F::RawVital => L::RawVital,
        F::Skill => L::Skill,
        F::RawSkill => L::RawSkill,
        F::SkillAdvancement => L::SkillAdvancement,
    }
}
pub(super) fn freeze_property_value(v: bace_entity::PropertyValue) -> value::NpcValueV1 {
    use bace_entity::PropertyValue as L;
    use value::NpcValueV1 as F;
    match v {
        L::Bool(v) => F::Bool(v),
        L::Int(v) => F::Int(v),
        L::Int64(v) => F::Int64(v),
        L::Float(v) => F::Float(v),
        L::String(v) => F::String(v),
        L::Unsigned(v) => F::Unsigned(v),
    }
}
pub(super) fn thaw_property_value(v: value::NpcValueV1) -> bace_entity::PropertyValue {
    use bace_entity::PropertyValue as L;
    use value::NpcValueV1 as F;
    match v {
        F::Bool(v) => L::Bool(v),
        F::Int(v) => L::Int(v),
        F::Int64(v) => L::Int64(v),
        F::Float(v) => L::Float(v),
        F::String(v) => L::String(v),
        F::Unsigned(v) => L::Unsigned(v),
    }
}
fn freeze_quest(v: bace_quests::QuestChange) -> frozen::NpcQuestChangeV1 {
    let progress = |v: bace_quests::QuestProgress| frozen::NpcQuestProgressV1 {
        completions: v.completions,
        last_completed: v.last_completed_seconds,
    };
    frozen::NpcQuestChangeV1 {
        name: v.name,
        before: v.before.map(progress),
        after: v.after.map(progress),
        before_revision: v.before_revision,
        after_revision: v.after_revision,
    }
}
fn thaw_quest(v: frozen::NpcQuestChangeV1) -> bace_quests::QuestChange {
    let progress = |v: frozen::NpcQuestProgressV1| bace_quests::QuestProgress {
        completions: v.completions,
        last_completed_seconds: v.last_completed,
    };
    bace_quests::QuestChange {
        name: v.name,
        before: v.before.map(progress),
        after: v.after.map(progress),
        before_revision: v.before_revision,
        after_revision: v.after_revision,
    }
}
fn freeze_experience(v: bace_character::ExperienceCredit) -> frozen::NpcExperienceCreditV1 {
    frozen::NpcExperienceCreditV1 {
        before_revision: v.before_revision,
        after_revision: v.after_revision,
        before_available: v.before_available,
        after_available: v.after_available,
    }
}
fn thaw_experience(v: frozen::NpcExperienceCreditV1) -> bace_character::ExperienceCredit {
    bace_character::ExperienceCredit {
        before_revision: v.before_revision,
        after_revision: v.after_revision,
        before_available: v.before_available,
        after_available: v.after_available,
    }
}
fn freeze_character(v: bace_character::CharacterServiceState) -> frozen::NpcCharacterStateV1 {
    frozen::NpcCharacterStateV1 {
        level: v.level,
        total_experience: v.total_experience,
        total_skill_credits: v.total_skill_credits,
        titles: v.titles,
        enlightenment: v.enlightenment,
        sanctuary: v.sanctuary.map(freeze_npc_destination),
    }
}
fn thaw_character(v: frozen::NpcCharacterStateV1) -> bace_character::CharacterServiceState {
    bace_character::CharacterServiceState {
        level: v.level,
        total_experience: v.total_experience,
        total_skill_credits: v.total_skill_credits,
        titles: v.titles,
        enlightenment: v.enlightenment,
        sanctuary: v.sanctuary.map(thaw_npc_destination),
    }
}
fn freeze_character_change(
    v: bace_character::CharacterServiceChange,
) -> frozen::NpcCharacterChangeV1 {
    frozen::NpcCharacterChangeV1 {
        before_revision: v.before_revision,
        after_revision: v.after_revision,
        before: freeze_character(v.before),
        after: freeze_character(v.after),
    }
}
fn thaw_character_change(
    v: frozen::NpcCharacterChangeV1,
) -> bace_character::CharacterServiceChange {
    bace_character::CharacterServiceChange {
        before_revision: v.before_revision,
        after_revision: v.after_revision,
        before: thaw_character(v.before),
        after: thaw_character(v.after),
    }
}
fn freeze_effect(v: NpcEffect) -> frozen::NpcEffectV1 {
    match v {
        NpcEffect::Property {
            actor,
            aggregate,
            change: c,
        } => frozen::NpcEffectV1::Property {
            actor: actor.0,
            aggregate: aggregate.map(|f| frozen::NpcAggregateFenceV1 {
                before_revision: f.before_revision,
                after_revision: f.after_revision,
            }),
            change: frozen::NpcPropertyChangeV1 {
                family: freeze_property_family(c.family),
                stat: c.stat,
                before: c.before.map(freeze_property_value),
                after: c.after.map(freeze_property_value),
                before_revision: c.before_revision,
                after_revision: c.after_revision,
            },
        },
        NpcEffect::Quest {
            actor,
            aggregate,
            change,
        } => frozen::NpcEffectV1::Quest {
            actor: actor.0,
            aggregate: aggregate.map(|f| frozen::NpcAggregateFenceV1 {
                before_revision: f.before_revision,
                after_revision: f.after_revision,
            }),
            change: freeze_quest(change),
        },
        NpcEffect::FellowQuest { fellowship, change } => frozen::NpcEffectV1::FellowQuest {
            fellowship,
            change: freeze_quest(change),
        },
        NpcEffect::Experience { actor, credit } => frozen::NpcEffectV1::Experience {
            actor: actor.0,
            credit: freeze_experience(credit),
        },
        NpcEffect::Luminance { actor, credit: c } => frozen::NpcEffectV1::Luminance {
            actor: actor.0,
            credit: frozen::NpcLuminanceCreditV1 {
                before_revision: c.before_revision,
                after_revision: c.after_revision,
                before_available: c.before.available,
                before_maximum: c.before.maximum,
                after_available: c.after.available,
                after_maximum: c.after.maximum,
                requested: c.requested,
            },
        },
        NpcEffect::CharacterService { actor, change } => frozen::NpcEffectV1::CharacterService {
            actor: actor.0,
            change: freeze_character_change(change),
        },
        NpcEffect::Contract {
            actor,
            before_revision,
            change,
        } => {
            let entry = |e: bace_quests::ContractState| bace_storage_codec::ContractSaveV1 {
                id: e.id,
                display: e.display,
            };
            frozen::NpcEffectV1::Contract {
                actor: actor.0,
                before_revision,
                change: frozen::NpcContractChangeV1 {
                    before: change.before.into_iter().map(entry).collect(),
                    after: change.after.into_iter().map(entry).collect(),
                },
            }
        }
        NpcEffect::EarnedExperience { actor, change: c } => frozen::NpcEffectV1::EarnedExperience {
            actor: actor.0,
            change: frozen::NpcEarnedExperienceV1 {
                experience: freeze_experience(c.experience),
                services: freeze_character_change(c.services),
                before_skill_credits: c.before_skill_credits,
                after_skill_credits: c.after_skill_credits,
                earned_skill_credits: c.earned_skill_credits,
                credited: c.credited,
            },
        },
        NpcEffect::QueuedExperience {
            actor,
            amount,
            phase,
            share,
        } => {
            let phase = match phase {
                bace_simulation::NpcQueuedExperiencePhase::AwaitingAdmission => {
                    frozen::NpcQueuedExperiencePhaseV1::AwaitingAdmission
                }
                bace_simulation::NpcQueuedExperiencePhase::Ready => {
                    frozen::NpcQueuedExperiencePhaseV1::Ready
                }
            };
            if share == bace_simulation::NpcExperienceSharing::All {
                frozen::NpcEffectV1::QueuedSharedExperience {
                    actor: actor.0,
                    amount,
                    phase,
                }
            } else {
                frozen::NpcEffectV1::QueuedExperience {
                    actor: actor.0,
                    amount,
                    phase,
                }
            }
        }
        NpcEffect::Service(op) => frozen::NpcEffectV1::Service(freeze_npc_operation(op)),
    }
}
fn thaw_effect(v: frozen::NpcEffectV1) -> NpcEffect {
    match v {
        frozen::NpcEffectV1::Property {
            actor,
            aggregate,
            change: c,
        } => NpcEffect::Property {
            actor: EntityId(actor),
            aggregate: aggregate.map(|f| bace_simulation::NpcAggregateFence {
                before_revision: f.before_revision,
                after_revision: f.after_revision,
            }),
            change: bace_entity::PropertyChange {
                family: thaw_property_family(c.family),
                stat: c.stat,
                before: c.before.map(thaw_property_value),
                after: c.after.map(thaw_property_value),
                before_revision: c.before_revision,
                after_revision: c.after_revision,
            },
        },
        frozen::NpcEffectV1::Quest {
            actor,
            aggregate,
            change,
        } => NpcEffect::Quest {
            actor: EntityId(actor),
            aggregate: aggregate.map(|f| bace_simulation::NpcAggregateFence {
                before_revision: f.before_revision,
                after_revision: f.after_revision,
            }),
            change: thaw_quest(change),
        },
        frozen::NpcEffectV1::FellowQuest { fellowship, change } => NpcEffect::FellowQuest {
            fellowship,
            change: thaw_quest(change),
        },
        frozen::NpcEffectV1::Experience { actor, credit } => NpcEffect::Experience {
            actor: EntityId(actor),
            credit: thaw_experience(credit),
        },
        frozen::NpcEffectV1::Luminance { actor, credit: c } => NpcEffect::Luminance {
            actor: EntityId(actor),
            credit: bace_character::LuminanceCredit {
                before_revision: c.before_revision,
                after_revision: c.after_revision,
                before: bace_character::LuminanceState {
                    available: c.before_available,
                    maximum: c.before_maximum,
                },
                after: bace_character::LuminanceState {
                    available: c.after_available,
                    maximum: c.after_maximum,
                },
                requested: c.requested,
            },
        },
        frozen::NpcEffectV1::CharacterService { actor, change } => NpcEffect::CharacterService {
            actor: EntityId(actor),
            change: thaw_character_change(change),
        },
        frozen::NpcEffectV1::Contract {
            actor,
            before_revision,
            change,
        } => {
            let entry = |e: bace_storage_codec::ContractSaveV1| bace_quests::ContractState {
                id: e.id,
                display: e.display,
            };
            NpcEffect::Contract {
                actor: EntityId(actor),
                before_revision,
                change: bace_quests::ContractChange {
                    before: change.before.into_iter().map(entry).collect(),
                    after: change.after.into_iter().map(entry).collect(),
                },
            }
        }
        frozen::NpcEffectV1::EarnedExperience { actor, change: c } => NpcEffect::EarnedExperience {
            actor: EntityId(actor),
            change: bace_character::EarnedExperienceChange {
                experience: thaw_experience(c.experience),
                services: thaw_character_change(c.services),
                before_skill_credits: c.before_skill_credits,
                after_skill_credits: c.after_skill_credits,
                earned_skill_credits: c.earned_skill_credits,
                credited: c.credited,
            },
        },
        frozen::NpcEffectV1::QueuedExperience {
            actor,
            amount,
            phase,
        } => NpcEffect::QueuedExperience {
            actor: EntityId(actor),
            amount,
            share: bace_simulation::NpcExperienceSharing::None,
            phase: match phase {
                frozen::NpcQueuedExperiencePhaseV1::AwaitingAdmission => {
                    bace_simulation::NpcQueuedExperiencePhase::AwaitingAdmission
                }
                frozen::NpcQueuedExperiencePhaseV1::Ready => {
                    bace_simulation::NpcQueuedExperiencePhase::Ready
                }
            },
        },
        frozen::NpcEffectV1::QueuedSharedExperience {
            actor,
            amount,
            phase,
        } => NpcEffect::QueuedExperience {
            actor: EntityId(actor),
            amount,
            share: bace_simulation::NpcExperienceSharing::All,
            phase: match phase {
                frozen::NpcQueuedExperiencePhaseV1::AwaitingAdmission => {
                    bace_simulation::NpcQueuedExperiencePhase::AwaitingAdmission
                }
                frozen::NpcQueuedExperiencePhaseV1::Ready => {
                    bace_simulation::NpcQueuedExperiencePhase::Ready
                }
            },
        },
        frozen::NpcEffectV1::Service(op) => NpcEffect::Service(thaw_npc_operation(op)),
    }
}
pub(super) fn freeze_pending(
    v: NpcPendingCheckpoint,
) -> Result<frozen::NpcPendingEffectV1, SaveCodecError> {
    let effect = freeze_effect(v.proposal.effect);
    frozen::validate_npc_effect(&effect)?;
    let frozen = frozen::NpcPendingEffectV1 {
        ticket: v.proposal.ticket,
        context: freeze_npc_context(v.proposal.context),
        effect,
        completion: freeze_npc_completion(v.completion),
        adopted: v.adopted,
        detached: v.detached,
    };
    frozen::validate_npc_pending_effect(&frozen)?;
    Ok(frozen)
}
pub(super) fn thaw_pending(
    v: frozen::NpcPendingEffectV1,
) -> Result<NpcPendingCheckpoint, SaveCodecError> {
    frozen::validate_npc_pending_effect(&v)?;
    Ok(NpcPendingCheckpoint {
        proposal: NpcProposal {
            ticket: v.ticket,
            context: thaw_npc_context(v.context),
            effect: thaw_effect(v.effect),
        },
        completion: thaw_npc_completion(v.completion),
        adopted: v.adopted,
        detached: v.detached,
    })
}
