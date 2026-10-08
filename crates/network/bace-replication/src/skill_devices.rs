//! Committed skill-device output from pinned SkillAlterationDevice.AlterSkill,
//! Player_Skills/RefundXP and AugmentationDevice.DoAugmentation. Consumption is
//! supplied by its accepted inventory owner and joins the same counter admission.
use crate::{
    BatchLimits, EventSequencer, InventoryProjection, ReplicationMessage, Sequences, SessionBatch,
    SessionProjectionError,
};
use bace_gameplay_api::{
    CharacterBinding, ProgressionTarget, SkillAdvancement, SkillTrainingChange,
};
use bace_types::EntityId;
use bace_wire::{CombatEffect, GroupEvent, ObjectCodecLimits, PropertyValue};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillDeviceKind {
    Specialize,
    Lower,
    Augment,
}
pub struct SkillDeviceView<'a> {
    pub kind: SkillDeviceKind,
    pub change: SkillTrainingChange,
    pub available_experience: u64,
    pub actor_name: &'a str,
    pub device_name: &'a str,
}
pub struct SkillDeviceBatch {
    pub owner: SessionBatch,
    /// Already included in owner output; route these same bytes to other local
    /// recipients without projecting again or consuming any counters.
    pub observers: Vec<ReplicationMessage>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "one atomic projection borrows the existing event, actor and item counter owners"
)]
pub fn project_skill_device(
    events: &mut EventSequencer,
    binding: CharacterBinding,
    view: SkillDeviceView<'_>,
    consumption: Vec<InventoryProjection<'_>>,
    items: &mut BTreeMap<EntityId, Sequences>,
    actor: &mut Sequences,
    objects: ObjectCodecLimits,
    limits: BatchLimits,
) -> Result<SkillDeviceBatch, SessionProjectionError> {
    use SkillAdvancement::{Specialized, Trained, Untrained};
    let invalid = SessionProjectionError::InvalidProjection;
    let change = view.change;
    let ProgressionTarget::Skill(skill) = change.after.target else {
        return Err(invalid);
    };
    if change.before.target != change.after.target
        || change.revision == 0
        || change.available_skill_credits > i32::MAX as u32
        || view.available_experience > i64::MAX as u64
        || skill == 0
        || skill > 54
        || consumption.is_empty()
        || consumption.len() > 16
    {
        return Err(invalid);
    }
    let name = crate::training_notice::NAMES[skill as usize];
    let classes = (change.before.advancement, change.after.advancement);
    let code = match (view.kind, classes) {
        (SkillDeviceKind::Specialize, (Trained, Specialized)) => 0x4d6,
        (SkillDeviceKind::Lower, (Specialized, Trained)) => 0x4d7,
        (SkillDeviceKind::Lower, (Specialized, Specialized)) => 0x55c,
        (SkillDeviceKind::Lower, (Trained, Untrained)) => 0x4d8,
        (SkillDeviceKind::Lower, (Trained, Trained)) => 0x4d9,
        (SkillDeviceKind::Augment, (Trained, Specialized)) => 0x55b,
        _ => return Err(invalid),
    };
    let augmentation = match skill {
        40 => 224,
        18 => 225,
        29 => 226,
        30 => 227,
        28 => 228,
        _ => 0,
    };
    if view.kind == SkillDeviceKind::Augment
        && (augmentation == 0 || view.actor_name.is_empty() || view.device_name.is_empty())
    {
        return Err(invalid);
    }
    if [view.actor_name.len(), view.device_name.len(), name.len()]
        .into_iter()
        .any(|n| n > limits.max_string_bytes)
    {
        return Err(SessionProjectionError::Limit);
    }
    let xp = || InventoryProjection::PrivateProperty {
        property: 2,
        value: PropertyValue::Int64(view.available_experience as i64),
    };
    let mut steps = Vec::with_capacity(consumption.len() + 8);
    if view.kind == SkillDeviceKind::Lower {
        steps.push(xp());
    }
    steps.push(InventoryProjection::Skill(change.after));
    let broadcast = format!(
        "{} has acquired the {} augmentation!",
        view.actor_name, view.device_name
    );
    if view.kind == SkillDeviceKind::Augment {
        steps.extend(consumption);
        steps.push(InventoryProjection::PrivateProperty {
            property: augmentation,
            value: PropertyValue::Int(1),
        });
        steps.push(xp());
        steps.push(InventoryProjection::Group(GroupEvent::ErrorWithString {
            code,
            text: view.device_name,
        }));
        steps.push(InventoryProjection::Effect(CombatEffect::Script {
            object_id: binding.actor.0,
            script_id: 0x9d,
            speed: 1.,
        }));
        steps.push(InventoryProjection::System {
            text: &broadcast,
            chat_type: 0,
        });
    } else {
        steps.push(InventoryProjection::PrivateProperty {
            property: 24,
            value: PropertyValue::Int(change.available_skill_credits as i32),
        });
        steps.push(InventoryProjection::Group(GroupEvent::ErrorWithString {
            code,
            text: name,
        }));
        steps.extend(consumption);
    }
    let owner = events.project_inventory_with_actor(
        binding,
        &steps,
        items,
        Some(actor),
        objects,
        limits,
    )?;
    let observers = if view.kind == SkillDeviceKind::Augment {
        owner.messages[owner.messages.len() - 2..].to_vec()
    } else {
        vec![]
    };
    Ok(SkillDeviceBatch { owner, observers })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillDeviceFailure {
    UnknownSkill,
    MustBeTrained,
    NotEnoughCredits,
    SpecializedCreditLimit,
    AlreadyUntrained,
    WieldRequirement,
    AugmentationNotEnoughExperience,
    AlreadyAugmented,
    AugmentationNotTrained,
    ConfirmationInProgress,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillDeviceNotice {
    pub code: u32,
    pub text: Option<String>,
}
/// Source failures only. Infrastructure/uncertain-save failures must retain the
/// operation rather than fabricate a gameplay rejection.
pub fn skill_device_failure(
    skill: u32,
    failure: SkillDeviceFailure,
) -> Result<SkillDeviceNotice, SessionProjectionError> {
    let name = crate::training_notice::NAMES
        .get(skill as usize)
        .ok_or(SessionProjectionError::InvalidProjection)?;
    use SkillDeviceFailure::*;
    let (code, text) = match failure {
        UnknownSkill => (0x4d0, None),
        MustBeTrained => (0x4d1, Some((*name).into())),
        NotEnoughCredits => (0x4d2, Some((*name).into())),
        SpecializedCreditLimit => (0x4da, Some((*name).into())),
        AlreadyUntrained => (0x4d4, Some((*name).into())),
        WieldRequirement => (0x4d5, Some((*name).into())),
        AugmentationNotEnoughExperience => (0x559, None),
        AlreadyAugmented => (0x557, None),
        AugmentationNotTrained => (
            0x55a,
            Some(format!(
                "You are not able to purchase this augmentation because you are not trained in {name}!"
            )),
        ),
        ConfirmationInProgress => (0x48f, None),
    };
    Ok(SkillDeviceNotice { code, text })
}

/// Pinned source uses Skill.ToSentence, not the DAT display label.
pub fn skill_device_prompt(
    skill: u32,
    kind: SkillDeviceKind,
    class: SkillAdvancement,
    specialize_cost: u32,
    device_name: &str,
    augmentation_cost: u64,
) -> Result<String, SessionProjectionError> {
    let name = crate::training_notice::NAMES
        .get(skill as usize)
        .filter(|_| skill != 0)
        .ok_or(SessionProjectionError::InvalidProjection)?;
    if device_name.len() > 4096 || augmentation_cost > i64::MAX as u64 {
        return Err(SessionProjectionError::InvalidProjection);
    }
    Ok(match kind {
        SkillDeviceKind::Specialize => format!(
            "This action will specialize your {name} skill and cost {specialize_cost} credits."
        ),
        SkillDeviceKind::Lower => format!(
            "This action will lower your {name} skill from {} and refund the skill credits and experience invested in this skill.",
            if class == SkillAdvancement::Specialized {
                "specialized to trained"
            } else {
                "trained to untrained"
            }
        ),
        SkillDeviceKind::Augment => {
            let digits = augmentation_cost.to_string();
            let mut number = String::with_capacity(digits.len() + 6);
            for (i, c) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i).is_multiple_of(3) {
                    number.push(',');
                }
                number.push(c);
            }
            format!(
                "This action will augment your character with {device_name} and will cost {number} available experience."
            )
        }
    })
}
