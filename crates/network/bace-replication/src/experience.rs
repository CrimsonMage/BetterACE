//! Official ACE Player_Xp.UpdateXpAndLevel/CheckForLevelup ordered publication.
//! Durable receipt adoption is required before constructing this accepted event.
use crate::{
    BatchLimits, ReplicationMessage, SequenceKind, Sequences, SessionBatch,
    SessionProjectionError as E,
};
use bace_gameplay_api::{CharacterBinding, experience::ExperienceEvent};
use bace_wire::{ChatMessage, CombatEffect, CurrentVitalUpdate, PropertyUpdate, PropertyValue};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperienceBatch {
    pub owner: SessionBatch,
    pub observers: Vec<ReplicationMessage>,
}
pub fn project_experience(
    binding: CharacterBinding,
    event: &ExperienceEvent,
    sequences: &mut Sequences,
    limits: BatchLimits,
) -> Result<ExperienceBatch, E> {
    if binding.actor != event.actor {
        return Err(E::WrongBinding);
    }
    let before = event.before;
    let after = event.after;
    if before.total > i64::MAX as u64
        || after.total > i64::MAX as u64
        || before.available > i64::MAX as u64
        || after.available > i64::MAX as u64
        || after.total < before.total
        || after.level < before.level
        || before.level == 0
        || event.maximum_level == 0
        || after.level > event.maximum_level
        || after.available_skill_credits > i32::MAX as u32
        || event.vitals.len() > 3
    {
        return Err(E::InvalidProjection);
    }
    let leveled = after.level > before.level;
    if !event.update_properties && (!event.vitals.is_empty() || leveled) {
        return Err(E::InvalidProjection);
    }
    if leveled
        && after.level < event.maximum_level
        && after.available_skill_credits == before.available_skill_credits
        && event.next_credit_level.is_none()
    {
        return Err(E::InvalidProjection);
    }
    let mut messages = vec![];
    let mut observers = vec![];
    let mut bytes = 0;
    let mut keys = vec![];
    let mut push =
        |queue, value| crate::session_output::push(&mut messages, &mut bytes, queue, value, limits);
    let next = |kind, key| (sequences.current(kind, key) as u8).wrapping_add(1);
    if event.update_properties && before.level < event.maximum_level {
        for (key, value) in [(1, after.total), (2, after.available)] {
            push(
                9,
                PropertyUpdate {
                    sequence: next(SequenceKind::PropertyInt64, key),
                    object_id: None,
                    property: key,
                    value: PropertyValue::Int64(value as i64),
                }
                .encode()?,
            )?;
            keys.push((SequenceKind::PropertyInt64, key));
        }
        if leveled {
            if after.level == event.maximum_level {
                let script = script(binding.actor.0, 0x8d, limits)?;
                push(10, script.clone())?;
                observers.push(ReplicationMessage {
                    queue: 10,
                    bytes: script,
                });
            }
            push(
                9,
                PropertyUpdate {
                    sequence: next(SequenceKind::PropertyInt, 25),
                    object_id: None,
                    property: 25,
                    value: PropertyValue::Int(after.level as i32),
                }
                .encode()?,
            )?;
            keys.push((SequenceKind::PropertyInt, 25));
            let mut seen = std::collections::BTreeSet::new();
            for &(vital, current) in &event.vitals {
                if ![2, 4, 6].contains(&vital) || !seen.insert(vital) {
                    return Err(E::InvalidProjection);
                }
                push(
                    9,
                    CurrentVitalUpdate {
                        sequence: next(SequenceKind::Vital, vital),
                        vital,
                        current,
                    }
                    .encode(),
                )?;
                keys.push((SequenceKind::Vital, vital));
            }
            let script = script(binding.actor.0, 0x8a, limits)?;
            push(10, script.clone())?;
            observers.push(ReplicationMessage {
                queue: 10,
                bytes: script,
            });
            let mut message = if after.level == event.maximum_level {
                format!("You have reached the maximum level of {}!", after.level)
            } else {
                format!("You are now level {}!", after.level)
            };
            message += &format!("\nYou have {} experience points", grouped(after.available));
            if after.available_skill_credits > 0 {
                message += &format!(" and {} skill credits", after.available_skill_credits);
            }
            message += " available to raise skills and attributes.";
            if after.level < event.maximum_level
                && after.available_skill_credits == before.available_skill_credits
            {
                message += &format!(
                    "\nYou will earn another skill credit at level {}.",
                    event.next_credit_level.ok_or(E::InvalidProjection)?
                );
            }
            if message.len() > limits.max_string_bytes {
                return Err(E::Limit);
            }
            push(
                9,
                ChatMessage::System {
                    text: &message,
                    chat_type: 13,
                }
                .encode()?,
            )?;
            push(
                9,
                PropertyUpdate {
                    sequence: next(SequenceKind::PropertyInt, 24),
                    object_id: None,
                    property: 24,
                    value: PropertyValue::Int(after.available_skill_credits as i32),
                }
                .encode()?,
            )?;
            keys.push((SequenceKind::PropertyInt, 24));
        } else if !event.vitals.is_empty() {
            return Err(E::InvalidProjection);
        }
    }
    if let Some(amount) = event.quest_amount {
        let text = format!("You've earned {} experience.", grouped(amount));
        if text.len() > limits.max_string_bytes {
            return Err(E::Limit);
        }
        push(
            9,
            ChatMessage::System {
                text: &text,
                chat_type: 0,
            }
            .encode()?,
        )?;
    }
    sequences.check_capacity(&keys).map_err(|_| E::Limit)?;
    for (kind, key) in keys {
        sequences
            .advance(kind, key)
            .expect("reserved sequence capacity");
    }
    Ok(ExperienceBatch {
        owner: SessionBatch { binding, messages },
        observers,
    })
}
fn script(actor: u32, id: u32, limits: BatchLimits) -> Result<Vec<u8>, E> {
    Ok(CombatEffect::Script {
        object_id: actor,
        script_id: id,
        speed: 1.,
    }
    .encode(limits.max_string_bytes, limits.max_message_bytes)?)
}
fn grouped(value: u64) -> String {
    let s = value.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}
