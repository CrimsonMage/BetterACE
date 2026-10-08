//! Pinned Player_Xp.GrantItemXP: item-owned private Int64 sequence, then level
//! message and AetheriaLevelUp script. The complete batch is capacity-checked.
use crate::{
    BatchLimits, ReplicationMessage, SequenceKind, Sequences, SessionBatch, SessionProjectionError,
};
use bace_gameplay_api::{CharacterBinding, item_experience::ItemExperienceEvent};
use bace_types::EntityId;
use bace_wire::{ChatMessage, CombatEffect, PropertyUpdate, PropertyValue};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemExperienceBatch {
    pub owner: SessionBatch,
    pub observers: Vec<ReplicationMessage>,
}
pub fn project_item_experience(
    binding: CharacterBinding,
    event: &ItemExperienceEvent,
    sequence_item: EntityId,
    item_sequences: &mut Sequences,
    limits: BatchLimits,
) -> Result<ItemExperienceBatch, SessionProjectionError> {
    if event.actor != binding.actor || event.item != sequence_item {
        return Err(SessionProjectionError::WrongBinding);
    }
    if event.item.0 == 0 || event.item == event.actor || event.total > i64::MAX as u64 {
        return Err(SessionProjectionError::InvalidProjection);
    }
    let mut messages = Vec::new();
    let mut total = 0;
    let sequence = (item_sequences.current(SequenceKind::PropertyInt64, 4) as u8).wrapping_add(1);
    crate::session_output::push(
        &mut messages,
        &mut total,
        9,
        PropertyUpdate {
            sequence,
            object_id: None,
            property: 4,
            value: PropertyValue::Int64(event.total as i64),
        }
        .encode()?,
        limits,
    )?;
    let mut observers = Vec::new();
    if let Some((name, level)) = &event.level_up {
        if name.len() > 1024 || *level == 0 || *level > 64 {
            return Err(SessionProjectionError::InvalidProjection);
        }
        let text = format!("Your {name} has increased in power to level {level}!");
        if text.len() > limits.max_string_bytes {
            return Err(SessionProjectionError::Limit);
        }
        crate::session_output::push(
            &mut messages,
            &mut total,
            9,
            ChatMessage::System {
                text: &text,
                chat_type: 0,
            }
            .encode()?,
            limits,
        )?;
        let script = CombatEffect::Script {
            object_id: binding.actor.0,
            script_id: 0xa1,
            speed: 1.,
        }
        .encode(limits.max_string_bytes, limits.max_message_bytes)?;
        crate::session_output::push(&mut messages, &mut total, 10, script.clone(), limits)?;
        observers.push(ReplicationMessage {
            queue: 10,
            bytes: script,
        });
    }
    item_sequences
        .advance_batch([(SequenceKind::PropertyInt64, 4)])
        .map_err(|_| SessionProjectionError::Limit)?;
    Ok(ItemExperienceBatch {
        owner: SessionBatch { binding, messages },
        observers,
    })
}
