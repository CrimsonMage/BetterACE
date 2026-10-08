//! Explicit conversion between native operations and frozen V1 vocabulary.
use bace_gameplay_api as live;
use bace_storage_codec::npc_values_v1 as frozen;
pub(super) fn freeze_npc_subject(value: live::NpcSubject) -> frozen::NpcSubjectV1 {
    match value {
        live::NpcSubject::Source => frozen::NpcSubjectV1::Source,
        live::NpcSubject::Target => frozen::NpcSubjectV1::Target,
        live::NpcSubject::Fellowship => frozen::NpcSubjectV1::Fellowship,
        live::NpcSubject::PetOwner => frozen::NpcSubjectV1::PetOwner,
        live::NpcSubject::ActivationTarget => frozen::NpcSubjectV1::ActivationTarget,
    }
}
pub(super) fn thaw_npc_subject(value: frozen::NpcSubjectV1) -> live::NpcSubject {
    match value {
        frozen::NpcSubjectV1::Source => live::NpcSubject::Source,
        frozen::NpcSubjectV1::Target => live::NpcSubject::Target,
        frozen::NpcSubjectV1::Fellowship => live::NpcSubject::Fellowship,
        frozen::NpcSubjectV1::PetOwner => live::NpcSubject::PetOwner,
        frozen::NpcSubjectV1::ActivationTarget => live::NpcSubject::ActivationTarget,
    }
}
pub(super) fn freeze_npc_context(value: live::NpcContext) -> frozen::NpcContextV1 {
    frozen::NpcContextV1 {
        source: value.source.0,
        target: value.target.map(|value| value.0),
        operation: value.operation,
    }
}
pub(super) fn thaw_npc_context(value: frozen::NpcContextV1) -> live::NpcContext {
    live::NpcContext {
        source: bace_types::EntityId(value.source),
        target: value.target.map(bace_types::EntityId),
        operation: value.operation,
    }
}
pub(super) fn freeze_npc_property_family(
    value: live::NpcPropertyFamily,
) -> frozen::NpcPropertyFamilyV1 {
    match value {
        live::NpcPropertyFamily::Bool => frozen::NpcPropertyFamilyV1::Bool,
        live::NpcPropertyFamily::Int => frozen::NpcPropertyFamilyV1::Int,
        live::NpcPropertyFamily::Int64 => frozen::NpcPropertyFamilyV1::Int64,
        live::NpcPropertyFamily::Float => frozen::NpcPropertyFamilyV1::Float,
        live::NpcPropertyFamily::String => frozen::NpcPropertyFamilyV1::String,
        live::NpcPropertyFamily::Attribute => frozen::NpcPropertyFamilyV1::Attribute,
        live::NpcPropertyFamily::RawAttribute => frozen::NpcPropertyFamilyV1::RawAttribute,
        live::NpcPropertyFamily::Vital => frozen::NpcPropertyFamilyV1::Vital,
        live::NpcPropertyFamily::RawVital => frozen::NpcPropertyFamilyV1::RawVital,
        live::NpcPropertyFamily::Skill => frozen::NpcPropertyFamilyV1::Skill,
        live::NpcPropertyFamily::RawSkill => frozen::NpcPropertyFamilyV1::RawSkill,
        live::NpcPropertyFamily::SkillAdvancement => frozen::NpcPropertyFamilyV1::SkillAdvancement,
    }
}
pub(super) fn thaw_npc_property_family(
    value: frozen::NpcPropertyFamilyV1,
) -> live::NpcPropertyFamily {
    match value {
        frozen::NpcPropertyFamilyV1::Bool => live::NpcPropertyFamily::Bool,
        frozen::NpcPropertyFamilyV1::Int => live::NpcPropertyFamily::Int,
        frozen::NpcPropertyFamilyV1::Int64 => live::NpcPropertyFamily::Int64,
        frozen::NpcPropertyFamilyV1::Float => live::NpcPropertyFamily::Float,
        frozen::NpcPropertyFamilyV1::String => live::NpcPropertyFamily::String,
        frozen::NpcPropertyFamilyV1::Attribute => live::NpcPropertyFamily::Attribute,
        frozen::NpcPropertyFamilyV1::RawAttribute => live::NpcPropertyFamily::RawAttribute,
        frozen::NpcPropertyFamilyV1::Vital => live::NpcPropertyFamily::Vital,
        frozen::NpcPropertyFamilyV1::RawVital => live::NpcPropertyFamily::RawVital,
        frozen::NpcPropertyFamilyV1::Skill => live::NpcPropertyFamily::Skill,
        frozen::NpcPropertyFamilyV1::RawSkill => live::NpcPropertyFamily::RawSkill,
        frozen::NpcPropertyFamilyV1::SkillAdvancement => live::NpcPropertyFamily::SkillAdvancement,
    }
}
pub(super) fn freeze_npc_value(value: live::NpcValue) -> frozen::NpcValueV1 {
    match value {
        live::NpcValue::Bool(inner) => frozen::NpcValueV1::Bool(inner),
        live::NpcValue::Int(inner) => frozen::NpcValueV1::Int(inner),
        live::NpcValue::Int64(inner) => frozen::NpcValueV1::Int64(inner),
        live::NpcValue::Float(inner) => frozen::NpcValueV1::Float(inner),
        live::NpcValue::String(inner) => frozen::NpcValueV1::String(inner),
        live::NpcValue::Unsigned(inner) => frozen::NpcValueV1::Unsigned(inner),
    }
}
pub(super) fn thaw_npc_value(value: frozen::NpcValueV1) -> live::NpcValue {
    match value {
        frozen::NpcValueV1::Bool(inner) => live::NpcValue::Bool(inner),
        frozen::NpcValueV1::Int(inner) => live::NpcValue::Int(inner),
        frozen::NpcValueV1::Int64(inner) => live::NpcValue::Int64(inner),
        frozen::NpcValueV1::Float(inner) => live::NpcValue::Float(inner),
        frozen::NpcValueV1::String(inner) => live::NpcValue::String(inner),
        frozen::NpcValueV1::Unsigned(inner) => live::NpcValue::Unsigned(inner),
    }
}
pub(super) fn freeze_npc_quest_mutation(
    value: live::NpcQuestMutation,
) -> frozen::NpcQuestMutationV1 {
    match value {
        live::NpcQuestMutation::Update => frozen::NpcQuestMutationV1::Update,
        live::NpcQuestMutation::Stamp => frozen::NpcQuestMutationV1::Stamp,
        live::NpcQuestMutation::Erase => frozen::NpcQuestMutationV1::Erase,
        live::NpcQuestMutation::Increment(inner) => frozen::NpcQuestMutationV1::Increment(inner),
        live::NpcQuestMutation::Decrement(inner) => frozen::NpcQuestMutationV1::Decrement(inner),
        live::NpcQuestMutation::Completions(inner) => {
            frozen::NpcQuestMutationV1::Completions(inner)
        }
        live::NpcQuestMutation::Bits { mask, on } => frozen::NpcQuestMutationV1::Bits { mask, on },
    }
}
pub(super) fn thaw_npc_quest_mutation(value: frozen::NpcQuestMutationV1) -> live::NpcQuestMutation {
    match value {
        frozen::NpcQuestMutationV1::Update => live::NpcQuestMutation::Update,
        frozen::NpcQuestMutationV1::Stamp => live::NpcQuestMutation::Stamp,
        frozen::NpcQuestMutationV1::Erase => live::NpcQuestMutation::Erase,
        frozen::NpcQuestMutationV1::Increment(inner) => live::NpcQuestMutation::Increment(inner),
        frozen::NpcQuestMutationV1::Decrement(inner) => live::NpcQuestMutation::Decrement(inner),
        frozen::NpcQuestMutationV1::Completions(inner) => {
            live::NpcQuestMutation::Completions(inner)
        }
        frozen::NpcQuestMutationV1::Bits { mask, on } => live::NpcQuestMutation::Bits { mask, on },
    }
}
pub(super) fn freeze_npc_property_mutation(
    value: live::NpcPropertyMutation,
) -> frozen::NpcPropertyMutationV1 {
    match value {
        live::NpcPropertyMutation::Set { family, value } => frozen::NpcPropertyMutationV1::Set {
            family: freeze_npc_property_family(family),
            value: value.map(freeze_npc_value),
        },
        live::NpcPropertyMutation::AddInt(inner) => frozen::NpcPropertyMutationV1::AddInt(inner),
    }
}
pub(super) fn thaw_npc_property_mutation(
    value: frozen::NpcPropertyMutationV1,
) -> live::NpcPropertyMutation {
    match value {
        frozen::NpcPropertyMutationV1::Set { family, value } => live::NpcPropertyMutation::Set {
            family: thaw_npc_property_family(family),
            value: value.map(thaw_npc_value),
        },
        frozen::NpcPropertyMutationV1::AddInt(inner) => live::NpcPropertyMutation::AddInt(inner),
    }
}
pub(super) fn freeze_npc_reward_kind(value: live::NpcRewardKind) -> frozen::NpcRewardKindV1 {
    match value {
        live::NpcRewardKind::Experience => frozen::NpcRewardKindV1::Experience,
        live::NpcRewardKind::NoShareExperience => frozen::NpcRewardKindV1::NoShareExperience,
        live::NpcRewardKind::LevelExperience => frozen::NpcRewardKindV1::LevelExperience,
        live::NpcRewardKind::SkillExperience => frozen::NpcRewardKindV1::SkillExperience,
        live::NpcRewardKind::SkillPoints => frozen::NpcRewardKindV1::SkillPoints,
        live::NpcRewardKind::LevelSkillExperience => frozen::NpcRewardKindV1::LevelSkillExperience,
        live::NpcRewardKind::TrainingCredits => frozen::NpcRewardKindV1::TrainingCredits,
        live::NpcRewardKind::Luminance => frozen::NpcRewardKindV1::Luminance,
        live::NpcRewardKind::SpendLuminance => frozen::NpcRewardKindV1::SpendLuminance,
        live::NpcRewardKind::Vitae => frozen::NpcRewardKindV1::Vitae,
        live::NpcRewardKind::RemoveVitae => frozen::NpcRewardKindV1::RemoveVitae,
        live::NpcRewardKind::Title => frozen::NpcRewardKindV1::Title,
        live::NpcRewardKind::TeachSpell => frozen::NpcRewardKindV1::TeachSpell,
        live::NpcRewardKind::UntrainSkill => frozen::NpcRewardKindV1::UntrainSkill,
        live::NpcRewardKind::Enlightenment => frozen::NpcRewardKindV1::Enlightenment,
    }
}
pub(super) fn thaw_npc_reward_kind(value: frozen::NpcRewardKindV1) -> live::NpcRewardKind {
    match value {
        frozen::NpcRewardKindV1::Experience => live::NpcRewardKind::Experience,
        frozen::NpcRewardKindV1::NoShareExperience => live::NpcRewardKind::NoShareExperience,
        frozen::NpcRewardKindV1::LevelExperience => live::NpcRewardKind::LevelExperience,
        frozen::NpcRewardKindV1::SkillExperience => live::NpcRewardKind::SkillExperience,
        frozen::NpcRewardKindV1::SkillPoints => live::NpcRewardKind::SkillPoints,
        frozen::NpcRewardKindV1::LevelSkillExperience => live::NpcRewardKind::LevelSkillExperience,
        frozen::NpcRewardKindV1::TrainingCredits => live::NpcRewardKind::TrainingCredits,
        frozen::NpcRewardKindV1::Luminance => live::NpcRewardKind::Luminance,
        frozen::NpcRewardKindV1::SpendLuminance => live::NpcRewardKind::SpendLuminance,
        frozen::NpcRewardKindV1::Vitae => live::NpcRewardKind::Vitae,
        frozen::NpcRewardKindV1::RemoveVitae => live::NpcRewardKind::RemoveVitae,
        frozen::NpcRewardKindV1::Title => live::NpcRewardKind::Title,
        frozen::NpcRewardKindV1::TeachSpell => live::NpcRewardKind::TeachSpell,
        frozen::NpcRewardKindV1::UntrainSkill => live::NpcRewardKind::UntrainSkill,
        frozen::NpcRewardKindV1::Enlightenment => live::NpcRewardKind::Enlightenment,
    }
}
pub(super) fn freeze_npc_text_kind(value: live::NpcTextKind) -> frozen::NpcTextKindV1 {
    match value {
        live::NpcTextKind::Act => frozen::NpcTextKindV1::Act,
        live::NpcTextKind::Say => frozen::NpcTextKindV1::Say,
        live::NpcTextKind::Tell => frozen::NpcTextKindV1::Tell,
        live::NpcTextKind::Direct => frozen::NpcTextKindV1::Direct,
        live::NpcTextKind::Local => frozen::NpcTextKindV1::Local,
        live::NpcTextKind::World => frozen::NpcTextKindV1::World,
        live::NpcTextKind::FellowBroadcast => frozen::NpcTextKindV1::FellowBroadcast,
        live::NpcTextKind::FellowTell => frozen::NpcTextKindV1::FellowTell,
        live::NpcTextKind::Admin => frozen::NpcTextKindV1::Admin,
        live::NpcTextKind::Log => frozen::NpcTextKindV1::Log,
        live::NpcTextKind::Popup => frozen::NpcTextKindV1::Popup,
    }
}
pub(super) fn thaw_npc_text_kind(value: frozen::NpcTextKindV1) -> live::NpcTextKind {
    match value {
        frozen::NpcTextKindV1::Act => live::NpcTextKind::Act,
        frozen::NpcTextKindV1::Say => live::NpcTextKind::Say,
        frozen::NpcTextKindV1::Tell => live::NpcTextKind::Tell,
        frozen::NpcTextKindV1::Direct => live::NpcTextKind::Direct,
        frozen::NpcTextKindV1::Local => live::NpcTextKind::Local,
        frozen::NpcTextKindV1::World => live::NpcTextKind::World,
        frozen::NpcTextKindV1::FellowBroadcast => live::NpcTextKind::FellowBroadcast,
        frozen::NpcTextKindV1::FellowTell => live::NpcTextKind::FellowTell,
        frozen::NpcTextKindV1::Admin => live::NpcTextKind::Admin,
        frozen::NpcTextKindV1::Log => live::NpcTextKind::Log,
        frozen::NpcTextKindV1::Popup => live::NpcTextKind::Popup,
    }
}
pub(super) fn freeze_npc_destination(value: live::NpcDestination) -> frozen::NpcDestinationV1 {
    frozen::NpcDestinationV1 {
        cell: value.cell.map(|value| value.0),
        position: [value.position.x, value.position.y, value.position.z],
        rotation: value.rotation,
        relative: value.relative,
    }
}
pub(super) fn thaw_npc_destination(value: frozen::NpcDestinationV1) -> live::NpcDestination {
    live::NpcDestination {
        cell: value.cell.map(bace_types::CellId),
        position: bace_geometry::Vec3::new(value.position[0], value.position[1], value.position[2]),
        rotation: value.rotation,
        relative: value.relative,
    }
}
pub(super) fn freeze_npc_operation(value: live::NpcOperation) -> frozen::NpcOperationV1 {
    match value {
        live::NpcOperation::Text { kind, text, extent } => frozen::NpcOperationV1::Text {
            kind: freeze_npc_text_kind(kind),
            text,
            extent,
        },
        live::NpcOperation::Property {
            subject,
            stat,
            mutation,
        } => frozen::NpcOperationV1::Property {
            subject: freeze_npc_subject(subject),
            stat,
            mutation: freeze_npc_property_mutation(mutation),
        },
        live::NpcOperation::Quest {
            subject,
            name,
            mutation,
        } => frozen::NpcOperationV1::Quest {
            subject: freeze_npc_subject(subject),
            name,
            mutation: freeze_npc_quest_mutation(mutation),
        },
        live::NpcOperation::Reward {
            kind,
            amount,
            stat,
            percent,
            minimum,
            maximum,
        } => frozen::NpcOperationV1::Reward {
            kind: freeze_npc_reward_kind(kind),
            amount,
            stat,
            percent,
            minimum,
            maximum,
        },
        live::NpcOperation::Give {
            template,
            count,
            palette,
            shade,
        } => frozen::NpcOperationV1::Give {
            template,
            count,
            palette,
            shade,
        },
        live::NpcOperation::Take { template, count } => {
            frozen::NpcOperationV1::Take { template, count }
        }
        live::NpcOperation::Cast {
            spell,
            instant,
            pet_owner,
        } => frozen::NpcOperationV1::Cast {
            spell,
            instant,
            pet_owner,
        },
        live::NpcOperation::Motion {
            target,
            motion,
            extent,
            style,
            substyle,
        } => frozen::NpcOperationV1::Motion {
            target,
            motion,
            extent,
            style,
            substyle,
        },
        live::NpcOperation::Move {
            home,
            absolute,
            destination,
            extent,
        } => frozen::NpcOperationV1::Move {
            home,
            absolute,
            destination: freeze_npc_destination(destination),
            extent,
        },
        live::NpcOperation::Turn { target, rotation } => {
            frozen::NpcOperationV1::Turn { target, rotation }
        }
        live::NpcOperation::TeleportTarget(inner) => {
            frozen::NpcOperationV1::TeleportTarget(freeze_npc_destination(inner))
        }
        live::NpcOperation::Sanctuary(inner) => {
            frozen::NpcOperationV1::Sanctuary(freeze_npc_destination(inner))
        }
        live::NpcOperation::ResetHome => frozen::NpcOperationV1::ResetHome,
        live::NpcOperation::Particle { script, extent } => {
            frozen::NpcOperationV1::Particle { script, extent }
        }
        live::NpcOperation::Sound(inner) => frozen::NpcOperationV1::Sound(inner),
        live::NpcOperation::Event { name, start } => frozen::NpcOperationV1::Event { name, start },
        live::NpcOperation::Signal(inner) => frozen::NpcOperationV1::Signal(inner),
        live::NpcOperation::Activate => frozen::NpcOperationV1::Activate,
        live::NpcOperation::Generate => frozen::NpcOperationV1::Generate,
        live::NpcOperation::DeleteSelf => frozen::NpcOperationV1::DeleteSelf,
        live::NpcOperation::KillSelf => frozen::NpcOperationV1::KillSelf,
        live::NpcOperation::OpenSelf(inner) => frozen::NpcOperationV1::OpenSelf(inner),
        live::NpcOperation::LockFellow { quest } => frozen::NpcOperationV1::LockFellow { quest },
        live::NpcOperation::Contract { id, add } => frozen::NpcOperationV1::Contract { id, add },
        live::NpcOperation::Barber => frozen::NpcOperationV1::Barber,
        live::NpcOperation::Treasure {
            tier,
            category,
            class,
        } => frozen::NpcOperationV1::Treasure {
            tier,
            category,
            class,
        },
        live::NpcOperation::Confirm { key, text } => frozen::NpcOperationV1::Confirm { key, text },
    }
}
pub(super) fn thaw_npc_operation(value: frozen::NpcOperationV1) -> live::NpcOperation {
    match value {
        frozen::NpcOperationV1::Text { kind, text, extent } => live::NpcOperation::Text {
            kind: thaw_npc_text_kind(kind),
            text,
            extent,
        },
        frozen::NpcOperationV1::Property {
            subject,
            stat,
            mutation,
        } => live::NpcOperation::Property {
            subject: thaw_npc_subject(subject),
            stat,
            mutation: thaw_npc_property_mutation(mutation),
        },
        frozen::NpcOperationV1::Quest {
            subject,
            name,
            mutation,
        } => live::NpcOperation::Quest {
            subject: thaw_npc_subject(subject),
            name,
            mutation: thaw_npc_quest_mutation(mutation),
        },
        frozen::NpcOperationV1::Reward {
            kind,
            amount,
            stat,
            percent,
            minimum,
            maximum,
        } => live::NpcOperation::Reward {
            kind: thaw_npc_reward_kind(kind),
            amount,
            stat,
            percent,
            minimum,
            maximum,
        },
        frozen::NpcOperationV1::Give {
            template,
            count,
            palette,
            shade,
        } => live::NpcOperation::Give {
            template,
            count,
            palette,
            shade,
        },
        frozen::NpcOperationV1::Take { template, count } => {
            live::NpcOperation::Take { template, count }
        }
        frozen::NpcOperationV1::Cast {
            spell,
            instant,
            pet_owner,
        } => live::NpcOperation::Cast {
            spell,
            instant,
            pet_owner,
        },
        frozen::NpcOperationV1::Motion {
            target,
            motion,
            extent,
            style,
            substyle,
        } => live::NpcOperation::Motion {
            target,
            motion,
            extent,
            style,
            substyle,
        },
        frozen::NpcOperationV1::Move {
            home,
            absolute,
            destination,
            extent,
        } => live::NpcOperation::Move {
            home,
            absolute,
            destination: thaw_npc_destination(destination),
            extent,
        },
        frozen::NpcOperationV1::Turn { target, rotation } => {
            live::NpcOperation::Turn { target, rotation }
        }
        frozen::NpcOperationV1::TeleportTarget(inner) => {
            live::NpcOperation::TeleportTarget(thaw_npc_destination(inner))
        }
        frozen::NpcOperationV1::Sanctuary(inner) => {
            live::NpcOperation::Sanctuary(thaw_npc_destination(inner))
        }
        frozen::NpcOperationV1::ResetHome => live::NpcOperation::ResetHome,
        frozen::NpcOperationV1::Particle { script, extent } => {
            live::NpcOperation::Particle { script, extent }
        }
        frozen::NpcOperationV1::Sound(inner) => live::NpcOperation::Sound(inner),
        frozen::NpcOperationV1::Event { name, start } => live::NpcOperation::Event { name, start },
        frozen::NpcOperationV1::Signal(inner) => live::NpcOperation::Signal(inner),
        frozen::NpcOperationV1::Activate => live::NpcOperation::Activate,
        frozen::NpcOperationV1::Generate => live::NpcOperation::Generate,
        frozen::NpcOperationV1::DeleteSelf => live::NpcOperation::DeleteSelf,
        frozen::NpcOperationV1::KillSelf => live::NpcOperation::KillSelf,
        frozen::NpcOperationV1::OpenSelf(inner) => live::NpcOperation::OpenSelf(inner),
        frozen::NpcOperationV1::LockFellow { quest } => live::NpcOperation::LockFellow { quest },
        frozen::NpcOperationV1::Contract { id, add } => live::NpcOperation::Contract { id, add },
        frozen::NpcOperationV1::Barber => live::NpcOperation::Barber,
        frozen::NpcOperationV1::Treasure {
            tier,
            category,
            class,
        } => live::NpcOperation::Treasure {
            tier,
            category,
            class,
        },
        frozen::NpcOperationV1::Confirm { key, text } => live::NpcOperation::Confirm { key, text },
    }
}
pub(super) fn freeze_npc_completion(value: live::NpcCompletion) -> frozen::NpcCompletionV1 {
    match value {
        live::NpcCompletion::Applied { post_delay } => {
            frozen::NpcCompletionV1::Applied { post_delay }
        }
        live::NpcCompletion::Pending { ticket } => frozen::NpcCompletionV1::Pending { ticket },
        live::NpcCompletion::Detached { ticket, post_delay } => {
            frozen::NpcCompletionV1::Detached { ticket, post_delay }
        }
        live::NpcCompletion::Branch { category } => frozen::NpcCompletionV1::Branch { category },
    }
}
pub(super) fn thaw_npc_completion(value: frozen::NpcCompletionV1) -> live::NpcCompletion {
    match value {
        frozen::NpcCompletionV1::Applied { post_delay } => {
            live::NpcCompletion::Applied { post_delay }
        }
        frozen::NpcCompletionV1::Pending { ticket } => live::NpcCompletion::Pending { ticket },
        frozen::NpcCompletionV1::Detached { ticket, post_delay } => {
            live::NpcCompletion::Detached { ticket, post_delay }
        }
        frozen::NpcCompletionV1::Branch { category } => live::NpcCompletion::Branch { category },
    }
}
