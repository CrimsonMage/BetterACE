//! Pinned AttributeTransferDevice success: two private attribute updates,
//! success WeenieError, then the accepted inventory consumption presentation.
//! Counters advance only after the whole bounded batch encodes.
use crate::{
    BatchLimits, EventSequencer, InventoryProjection, ReplicationMessage, SequenceKind, Sequences,
    SessionBatch, SessionProjectionError,
};
use bace_gameplay_api::{CharacterBinding, ProgressionProjection, ProgressionTarget, TraitDetails};
use bace_types::EntityId;
use bace_wire::{AttributeUpdate, ObjectCodecLimits, SimpleGameEvent};
use std::collections::BTreeMap;

#[expect(
    clippy::too_many_arguments,
    reason = "one atomic projection borrows the existing event, actor and item counter owners"
)]
pub fn project_attribute_transfer(
    events: &mut EventSequencer,
    binding: CharacterBinding,
    from: ProgressionProjection,
    to: ProgressionProjection,
    consumption: Vec<InventoryProjection<'_>>,
    items: &mut BTreeMap<EntityId, Sequences>,
    actor: &mut Sequences,
    objects: ObjectCodecLimits,
    limits: BatchLimits,
) -> Result<SessionBatch, SessionProjectionError> {
    let attribute =
        |projection: ProgressionProjection| match (projection.target, projection.details) {
            (
                ProgressionTarget::Attribute(id),
                Some(TraitDetails::Attribute { starting_value }),
            ) if (10..=100).contains(&starting_value) => Ok((id as u32, starting_value)),
            _ => Err(SessionProjectionError::InvalidProjection),
        };
    let (from_id, from_start) = attribute(from)?;
    let (to_id, to_start) = attribute(to)?;
    if consumption.is_empty() || consumption.len() > 16 || binding.actor.0 == 0 {
        return Err(SessionProjectionError::InvalidProjection);
    }
    let mut staged_actor = actor.proposal_copy();
    let mut staged_items: BTreeMap<_, _> = items
        .iter()
        .map(|(id, counter)| (*id, counter.proposal_copy()))
        .collect();
    let mut staged_events = EventSequencer {
        binding: events.binding,
        next: events.next,
    };
    let [from_sequence, to_sequence] = staged_actor
        .advance_batch([
            (SequenceKind::Attribute, from_id),
            (SequenceKind::Attribute, to_id),
        ])
        .map_err(|_| SessionProjectionError::Limit)?;
    let mut messages = Vec::with_capacity(consumption.len() + 3);
    for (projection, id, starting, sequence) in [
        (from, from_id, from_start, from_sequence),
        (to, to_id, to_start, to_sequence),
    ] {
        messages.push(ReplicationMessage {
            queue: 9,
            bytes: AttributeUpdate {
                sequence: sequence as u8,
                attribute: id,
                ranks: projection.ranks.into(),
                starting_value: starting,
                experience_spent: projection.experience_spent,
            }
            .encode(),
        });
    }
    let mut steps = Vec::with_capacity(consumption.len() + 1);
    steps.push(InventoryProjection::Simple(SimpleGameEvent::WeenieError(
        0x04e1,
    )));
    steps.extend(consumption);
    let batch = staged_events.project_inventory_with_actor(
        binding,
        &steps,
        &mut staged_items,
        Some(&mut staged_actor),
        objects,
        limits,
    )?;
    messages.extend(batch.messages);
    if messages.len() > limits.max_messages
        || messages
            .iter()
            .map(|message| message.bytes.len())
            .sum::<usize>()
            > limits.max_bytes
        || messages
            .iter()
            .any(|message| message.bytes.len() > limits.max_message_bytes)
    {
        return Err(SessionProjectionError::Limit);
    }
    *actor = staged_actor;
    *items = staged_items;
    events.next = staged_events.next;
    Ok(SessionBatch { binding, messages })
}
