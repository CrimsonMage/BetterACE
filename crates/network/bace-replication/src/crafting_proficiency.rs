//! Atomic compound recipe output: item effects, immediate skill spend, UseDone,
//! then ACE's queued proficiency XP grant. Temporary counters are never live owners.
use crate::{
    BatchLimits, EventSequencer, InventoryProjection, ReplicationMessage, SequenceKind, Sequences,
    SessionBatch, SessionProjectionError as E,
};
use bace_gameplay_api::{
    CharacterBinding, ProgressionProjection, ProgressionTarget, TraitDetails,
    experience::ExperienceEvent,
};
use bace_types::EntityId;
use bace_wire::{
    CombatEffect, ObjectCodecLimits, PropertyUpdate, PropertyValue, SimpleGameEvent, SkillUpdate,
};
use std::collections::{BTreeMap, BTreeSet};
pub struct CraftingSkillSpend {
    pub after: ProgressionProjection,
    pub available: u64,
    pub rank_changed: bool,
    pub base: Option<u32>,
    pub maximum: bool,
}
pub struct CraftingProficiencyProjection<'a> {
    pub skill: Option<CraftingSkillSpend>,
    pub experience: Option<&'a ExperienceEvent>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "One atomic bounded batch joins existing session, actor and two item counter owners"
)]
pub fn project_crafting_commit(
    sequencer: &mut EventSequencer,
    binding: CharacterBinding,
    steps: &[InventoryProjection<'_>],
    items: &mut BTreeMap<EntityId, Sequences>,
    actor: &mut Sequences,
    proficiency: Option<CraftingProficiencyProjection<'_>>,
    use_done: bool,
    objects: ObjectCodecLimits,
    limits: BatchLimits,
) -> Result<crate::experience::ExperienceBatch, E> {
    let ids = steps
        .iter()
        .filter_map(|step| match step {
            InventoryProjection::Stack { item, .. }
            | InventoryProjection::Property { item, .. }
            | InventoryProjection::Container { item, .. }
            | InventoryProjection::Position { item, .. }
                if *item != binding.actor =>
            {
                Some(*item)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if ids.len() > 2 {
        return Err(E::Limit);
    }
    let mut next_items = ids
        .iter()
        .map(|id| {
            Ok((
                *id,
                items.get(id).ok_or(E::InvalidProjection)?.proposal_copy(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, E>>()?;
    let mut next_actor = actor.proposal_copy();
    let mut next_events = EventSequencer {
        binding: sequencer.binding,
        next: sequencer.next,
    };
    let mut batch = next_events.project_inventory_with_actor(
        binding,
        steps,
        &mut next_items,
        Some(&mut next_actor),
        objects,
        limits,
    )?;
    let mut bytes = batch.messages.iter().map(|m| m.bytes.len()).sum();
    let mut observers = Vec::new();
    if let Some(skill) = proficiency.as_ref().and_then(|p| p.skill.as_ref()) {
        let ProgressionTarget::Skill(id) = skill.after.target else {
            return Err(E::InvalidProjection);
        };
        let Some(TraitDetails::Skill {
            initial_level,
            resistance_at_last_check,
            last_used_time,
        }) = skill.after.details
        else {
            return Err(E::InvalidProjection);
        };
        if !last_used_time.is_finite() || skill.available > i64::MAX as u64 {
            return Err(E::InvalidProjection);
        }
        let [xp, seq] = next_actor
            .advance_batch([(SequenceKind::PropertyInt64, 2), (SequenceKind::Skill, id)])
            .map_err(|_| E::Limit)?;
        crate::session_output::push(
            &mut batch.messages,
            &mut bytes,
            9,
            PropertyUpdate {
                sequence: xp as u8,
                object_id: None,
                property: 2,
                value: PropertyValue::Int64(skill.available as i64),
            }
            .encode()?,
            limits,
        )?;
        crate::session_output::push(
            &mut batch.messages,
            &mut bytes,
            9,
            SkillUpdate {
                sequence: seq as u8,
                skill: id,
                ranks: skill.after.ranks,
                advancement_class: skill.after.advancement as u32,
                experience_spent: skill.after.experience_spent,
                initial_level,
                resistance_at_last_check,
                last_used_time,
            }
            .encode(),
            limits,
        )?;
        if skill.rank_changed {
            if skill.maximum {
                let script = CombatEffect::Script {
                    object_id: binding.actor.0,
                    script_id: 0x8d,
                    speed: 1.0,
                }
                .encode(limits.max_string_bytes, limits.max_message_bytes)?;
                observers.push(ReplicationMessage {
                    queue: 10,
                    bytes: script.clone(),
                });
                crate::session_output::push(&mut batch.messages, &mut bytes, 10, script, limits)?;
            }
            crate::session_output::push(
                &mut batch.messages,
                &mut bytes,
                10,
                CombatEffect::Sound {
                    object_id: binding.actor.0,
                    sound_id: 0x8b,
                    volume: 1.0,
                }
                .encode(limits.max_string_bytes, limits.max_message_bytes)?,
                limits,
            )?;
            let name = crate::training_notice::NAMES
                .get(id as usize)
                .ok_or(E::InvalidProjection)?;
            let base = skill.base.ok_or(E::InvalidProjection)?;
            let suffix = if skill.maximum {
                " and has reached its upper limit"
            } else {
                ""
            };
            let text = format!("Your base {name} skill is now {base}{suffix}!");
            crate::session_output::push(
                &mut batch.messages,
                &mut bytes,
                9,
                bace_wire::ChatMessage::System {
                    text: &text,
                    chat_type: 0x0d,
                }
                .encode()?,
                limits,
            )?;
        }
    }
    if use_done {
        let end = next_events.project_inventory(
            binding,
            &[InventoryProjection::Simple(SimpleGameEvent::UseDone(0))],
            &mut next_items,
            objects,
            limits,
        )?;
        for m in end.messages {
            crate::session_output::push(&mut batch.messages, &mut bytes, m.queue, m.bytes, limits)?;
        }
    }
    if let Some(event) = proficiency.and_then(|p| p.experience) {
        let xp = crate::experience::project_experience(binding, event, &mut next_actor, limits)?;
        for m in xp.owner.messages {
            crate::session_output::push(&mut batch.messages, &mut bytes, m.queue, m.bytes, limits)?;
        }
        observers.extend(xp.observers);
    }
    for (id, next) in next_items {
        *items.get_mut(&id).ok_or(E::InvalidProjection)? = next;
    }
    *actor = next_actor;
    sequencer.next = next_events.next;
    Ok(crate::experience::ExperienceBatch {
        owner: SessionBatch {
            binding,
            messages: batch.messages,
        },
        observers,
    })
}
