//! Committed inventory messages share the canonical session and object counters.
use crate::{
    BatchLimits, EventSequencer, SequenceKind, Sequences, SessionBatch, SessionProjectionError,
};
use bace_gameplay_api::CharacterBinding;
use bace_types::EntityId;
use bace_wire::{InventoryEvent, ObjectCodecLimits, ObjectControl, ObjectDescription};
use std::collections::{BTreeMap, BTreeSet};
pub enum InventoryProjection<'a> {
    Skill(bace_gameplay_api::ProgressionProjection),
    PrivateProperty {
        property: u32,
        value: bace_wire::PropertyValue<'a>,
    },
    PrivatePosition {
        position_type: u32,
        position: bace_wire::WirePosition,
    },
    Appearance {
        item: EntityId,
        model: &'a bace_wire::ObjectModel,
    },
    Parent {
        parent: EntityId,
        item: EntityId,
        location: u32,
        placement: u32,
    },
    Pickup(EntityId),
    Vital {
        vital: u32,
        current: u32,
    },
    Magic(bace_wire::MagicEvent<'a>),
    Effect(bace_wire::CombatEffect<'a>),
    Group(bace_wire::GroupEvent<'a>),
    Social(bace_wire::SocialEvent<'a>),
    Create(&'a ObjectDescription),
    Update(&'a ObjectDescription),
    Simple(bace_wire::SimpleGameEvent),
    Crafting(bace_wire::CraftingEvent<'a>),
    System {
        text: &'a str,
        chat_type: u32,
    },
    Event(InventoryEvent<'a>),
    Stack {
        item: EntityId,
        quantity: u32,
        value: u32,
    },
    Property {
        item: EntityId,
        property: u32,
        value: bace_wire::PropertyValue<'a>,
    },
    Remove(EntityId),
    /// ACE GameMessageDeleteObject for an item removed from the player's
    /// smartbox (distinct from InventoryRemoveObject).
    Delete(EntityId),
    Container {
        item: EntityId,
        value: u32,
    },
    Position {
        item: EntityId,
        pack: bace_wire::PositionPack,
    },
}
impl EventSequencer {
    /// Encode the entire bounded batch before advancing any sequence. The caller
    /// retains this batch through reliable-channel pressure; never regenerate it.
    pub fn project_inventory(
        &mut self,
        binding: CharacterBinding,
        steps: &[InventoryProjection<'_>],
        items: &mut BTreeMap<EntityId, Sequences>,
        objects: ObjectCodecLimits,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.project_inventory_with_actor(binding, steps, items, None, objects, limits)
    }
    /// Same atomic batch, with the existing character property owner available
    /// for recipe effects addressing the actor. No counter owner is duplicated.
    pub fn project_inventory_with_actor(
        &mut self,
        binding: CharacterBinding,
        steps: &[InventoryProjection<'_>],
        items: &mut BTreeMap<EntityId, Sequences>,
        mut actor: Option<&mut Sequences>,
        objects: ObjectCodecLimits,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if steps.len() > 4096 || steps.len() > limits.max_messages {
            return Err(SessionProjectionError::Limit);
        }
        let mut keys = BTreeSet::new();
        let mut strict = BTreeSet::new();
        let mut increments: BTreeMap<(EntityId, SequenceKind, u32), usize> = BTreeMap::new();
        for step in steps {
            if let InventoryProjection::PrivatePosition {
                position_type,
                position,
            } = step
            {
                if *position_type == 0
                    || position.cell == 0
                    || position
                        .origin
                        .iter()
                        .chain(&position.rotation)
                        .any(|v| !v.is_finite())
                    || (position.rotation.iter().map(|v| v * v).sum::<f32>() - 1.).abs() > 0.001
                {
                    return Err(SessionProjectionError::InvalidProjection);
                }
                let key = (binding.actor, SequenceKind::Position, *position_type);
                keys.insert(key);
                *increments.entry(key).or_default() += 1;
                continue;
            }
            if let InventoryProjection::PrivateProperty { property, value } = step {
                if *property == 0 {
                    return Err(SessionProjectionError::InvalidProjection);
                }
                let key = (binding.actor, property_kind(value), *property);
                keys.insert(key);
                *increments.entry(key).or_default() += 1;
                continue;
            }
            if let InventoryProjection::Property {
                item,
                property,
                value,
            } = step
            {
                if *property == 0 {
                    return Err(SessionProjectionError::InvalidProjection);
                }
                let key = (*item, property_kind(value), *property);
                keys.insert(key);
                *increments.entry(key).or_default() += 1;
                continue;
            }
            let key = match step {
                InventoryProjection::Skill(projection) => {
                    let bace_gameplay_api::ProgressionTarget::Skill(skill) = projection.target
                    else {
                        return Err(SessionProjectionError::InvalidProjection);
                    };
                    if !(1..=54).contains(&skill) {
                        return Err(SessionProjectionError::InvalidProjection);
                    }
                    Some((binding.actor, SequenceKind::Skill, skill))
                }
                InventoryProjection::Appearance { item, .. } => {
                    Some((*item, SequenceKind::ObjectVisualDesc, 0))
                }
                InventoryProjection::Parent { item, .. } | InventoryProjection::Pickup(item) => {
                    Some((*item, SequenceKind::ObjectPosition, 0))
                }
                InventoryProjection::Vital { vital, .. } => {
                    if !matches!(vital, 2 | 4 | 6) {
                        return Err(SessionProjectionError::InvalidProjection);
                    }
                    Some((binding.actor, SequenceKind::Vital, *vital))
                }

                InventoryProjection::Stack { item, .. } => {
                    Some((*item, SequenceKind::PropertyInt, 12))
                }
                InventoryProjection::Container { item, .. } => {
                    Some((*item, SequenceKind::PropertyInstanceId, 2))
                }
                InventoryProjection::Position { item, pack } => {
                    if pack.position.cell == 0
                        || pack
                            .position
                            .origin
                            .into_iter()
                            .chain(pack.position.rotation)
                            .chain(pack.velocity.into_iter().flatten())
                            .any(|v| !v.is_finite())
                        || (pack
                            .position
                            .rotation
                            .into_iter()
                            .map(|v| v * v)
                            .sum::<f32>()
                            - 1.)
                            .abs()
                            > 0.001
                    {
                        return Err(SessionProjectionError::InvalidProjection);
                    }
                    Some((*item, SequenceKind::ObjectPosition, 0))
                }
                _ => None,
            };
            if let Some(key) = key {
                if !strict.insert(key) {
                    return Err(SessionProjectionError::InvalidProjection);
                }
                keys.insert(key);
                *increments.entry(key).or_default() += 1;
            }
        }
        let mut capacity: BTreeMap<EntityId, Vec<(SequenceKind, u32)>> = BTreeMap::new();
        for &(id, kind, property) in &keys {
            capacity.entry(id).or_default().push((kind, property));
        }
        for (id, keys) in capacity {
            owner(items, actor.as_deref(), binding.actor, id)?
                .check_capacity(&keys)
                .map_err(|_| SessionProjectionError::Limit)?;
        }
        let mut messages = Vec::new();
        let mut total = 0;
        let mut next = self.next;
        let mut offsets = BTreeMap::new();
        for step in steps {
            let (queue, bytes) = match step {
                InventoryProjection::PrivatePosition {
                    position_type,
                    position,
                } => {
                    let key = (binding.actor, SequenceKind::Position, *position_type);
                    let offset = offsets.entry(key).or_insert(0u8);
                    *offset = offset.wrapping_add(1);
                    (
                        9,
                        bace_wire::PrivatePositionUpdate {
                            sequence:
                                (owner(items, actor.as_deref(), binding.actor, binding.actor)?
                                    .current(SequenceKind::Position, *position_type)
                                    as u8)
                                    .wrapping_add(*offset),
                            position_type: *position_type,
                            position: *position,
                        }
                        .encode(),
                    )
                }
                InventoryProjection::PrivateProperty { property, value } => {
                    if let bace_wire::PropertyValue::String(text) = value
                        && text.len() > limits.max_string_bytes
                    {
                        return Err(SessionProjectionError::Limit);
                    }
                    let kind = property_kind(value);
                    let key = (binding.actor, kind, *property);
                    let offset = offsets.entry(key).or_insert(0u8);
                    *offset = offset.wrapping_add(1);
                    (
                        9,
                        bace_wire::PropertyUpdate {
                            sequence:
                                (owner(items, actor.as_deref(), binding.actor, binding.actor)?
                                    .current(kind, *property)
                                    as u8)
                                    .wrapping_add(*offset),
                            object_id: None,
                            property: *property,
                            value: value.clone(),
                        }
                        .encode()?,
                    )
                }
                InventoryProjection::Skill(projection) => {
                    let bace_gameplay_api::ProgressionTarget::Skill(skill) = projection.target
                    else {
                        return Err(SessionProjectionError::InvalidProjection);
                    };
                    let Some(bace_gameplay_api::TraitDetails::Skill {
                        initial_level,
                        resistance_at_last_check,
                        last_used_time,
                    }) = projection.details
                    else {
                        return Err(SessionProjectionError::InvalidProjection);
                    };
                    if !last_used_time.is_finite() {
                        return Err(SessionProjectionError::InvalidProjection);
                    }
                    (
                        9,
                        bace_wire::SkillUpdate {
                            sequence:
                                (owner(items, actor.as_deref(), binding.actor, binding.actor)?
                                    .current(SequenceKind::Skill, skill)
                                    as u8)
                                    .wrapping_add(1),
                            skill,
                            ranks: projection.ranks,
                            advancement_class: projection.advancement as u32,
                            experience_spent: projection.experience_spent,
                            initial_level,
                            resistance_at_last_check,
                            last_used_time,
                        }
                        .encode(),
                    )
                }
                InventoryProjection::Property {
                    item,
                    property,
                    value,
                } => {
                    if let bace_wire::PropertyValue::String(text) = value
                        && text.len() > limits.max_string_bytes
                    {
                        return Err(SessionProjectionError::Limit);
                    }
                    let key = (*item, property_kind(value), *property);
                    let offset = offsets.entry(key).or_insert(0u8);
                    *offset = offset.wrapping_add(1);
                    (
                        9,
                        bace_wire::PropertyUpdate {
                            sequence: (owner(items, actor.as_deref(), binding.actor, *item)?
                                .current(key.1, *property)
                                as u8)
                                .wrapping_add(*offset),
                            object_id: Some(item.0),
                            property: *property,
                            value: value.clone(),
                        }
                        .encode()?,
                    )
                }
                InventoryProjection::Appearance { item, model } => (
                    10,
                    bace_wire::AppearanceUpdate {
                        object_id: item.0,
                        model: (*model).clone(),
                        instance_sequence: owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectInstance, 0),
                        visual_sequence: owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectVisualDesc, 0)
                            .wrapping_add(1),
                    }
                    .encode(objects.max_model_entries)?,
                ),
                InventoryProjection::Parent {
                    parent,
                    item,
                    location,
                    placement,
                } => (
                    10,
                    ObjectControl::Parent {
                        parent_id: parent.0,
                        child_id: item.0,
                        parent_location: *location,
                        placement: *placement,
                        instance_sequence: owner(items, actor.as_deref(), binding.actor, *parent)?
                            .current(SequenceKind::ObjectInstance, 0),
                        position_sequence: owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectPosition, 0)
                            .wrapping_add(1),
                    }
                    .encode(),
                ),
                InventoryProjection::Pickup(item) => (
                    10,
                    ObjectControl::Pickup {
                        object_id: item.0,
                        instance_sequence: owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectInstance, 0),
                        position_sequence: owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectPosition, 0)
                            .wrapping_add(1),
                    }
                    .encode(),
                ),
                InventoryProjection::Vital { vital, current } => (
                    9,
                    bace_wire::CurrentVitalUpdate {
                        sequence: (owner(items, actor.as_deref(), binding.actor, binding.actor)?
                            .current(SequenceKind::Vital, *vital)
                            as u8)
                            .wrapping_add(1),
                        vital: *vital,
                        current: *current,
                    }
                    .encode(),
                ),
                InventoryProjection::Effect(effect) => (
                    if matches!(
                        effect,
                        bace_wire::CombatEffect::Script { .. }
                            | bace_wire::CombatEffect::Sound { .. }
                    ) {
                        10
                    } else {
                        9
                    },
                    effect.encode(limits.max_string_bytes, limits.max_message_bytes)?,
                ),
                InventoryProjection::Magic(event) => {
                    let bytes =
                        event.encode(binding.actor.0, next, 4096, limits.max_message_bytes)?;
                    next = next.wrapping_add(1);
                    (9, bytes)
                }
                InventoryProjection::Group(event) => {
                    let bytes = event.encode(binding.actor.0, next, social_limits(limits))?;
                    next = next.wrapping_add(1);
                    (9, bytes)
                }
                InventoryProjection::Social(event) => {
                    let bytes = event.encode(binding.actor.0, next, social_limits(limits))?;
                    next = next.wrapping_add(1);
                    (9, bytes)
                }
                InventoryProjection::Create(object) => (10, object.encode_create(objects)?),
                InventoryProjection::Update(object) => (10, object.encode_update(objects)?),
                InventoryProjection::Crafting(event) => {
                    let bytes = event.encode(binding.actor.0, next, limits.max_message_bytes)?;
                    next = next.wrapping_add(1);
                    (9, bytes)
                }
                InventoryProjection::Simple(event) => {
                    let bytes = event.encode(binding.actor.0, next);
                    next = next.wrapping_add(1);
                    (9, bytes)
                }
                InventoryProjection::System { text, chat_type } => {
                    if text.len() > limits.max_string_bytes {
                        return Err(SessionProjectionError::Limit);
                    }
                    (
                        9,
                        bace_wire::ChatMessage::System {
                            text,
                            chat_type: *chat_type,
                        }
                        .encode()?,
                    )
                }
                InventoryProjection::Event(event) => {
                    let bytes =
                        event.encode(binding.actor.0, next, 1024, limits.max_message_bytes)?;
                    next = next.wrapping_add(1);
                    (9, bytes)
                }
                InventoryProjection::Stack {
                    item,
                    quantity,
                    value,
                } => {
                    let offset = offsets
                        .entry((*item, SequenceKind::PropertyInt, 12))
                        .or_insert(0u8);
                    *offset = offset.wrapping_add(1);
                    let sequence = (owner(items, actor.as_deref(), binding.actor, *item)?
                        .current(SequenceKind::PropertyInt, 12)
                        as u8)
                        .wrapping_add(*offset);
                    (
                        9,
                        ObjectControl::StackSize {
                            object_id: item.0,
                            sequence,
                            stack_size: *quantity,
                            value: *value,
                        }
                        .encode(),
                    )
                }
                InventoryProjection::Container { item, value } => {
                    let offset = offsets
                        .entry((*item, SequenceKind::PropertyInstanceId, 2))
                        .or_insert(0u8);
                    *offset = offset.wrapping_add(1);
                    (
                        9,
                        bace_wire::PropertyUpdate {
                            sequence: (owner(items, actor.as_deref(), binding.actor, *item)?
                                .current(SequenceKind::PropertyInstanceId, 2)
                                as u8)
                                .wrapping_add(*offset),
                            object_id: Some(item.0),
                            property: 2,
                            value: bace_wire::PropertyValue::InstanceId(*value),
                        }
                        .encode()?,
                    )
                }
                InventoryProjection::Position { item, pack } => {
                    let mut pack = *pack;
                    pack.instance_sequence = owner(items, actor.as_deref(), binding.actor, *item)?
                        .current(SequenceKind::ObjectInstance, 0);
                    pack.position_sequence = owner(items, actor.as_deref(), binding.actor, *item)?
                        .current(SequenceKind::ObjectPosition, 0)
                        .wrapping_add(1);
                    pack.teleport_sequence = owner(items, actor.as_deref(), binding.actor, *item)?
                        .current(SequenceKind::ObjectTeleport, 0);
                    pack.force_position_sequence =
                        owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectForcePosition, 0);
                    (
                        10,
                        bace_wire::PositionUpdate {
                            object_id: item.0,
                            pack,
                        }
                        .encode(),
                    )
                }
                InventoryProjection::Remove(item) => (
                    9,
                    ObjectControl::InventoryRemove { object_id: item.0 }.encode(),
                ),
                InventoryProjection::Delete(item) => (
                    10,
                    ObjectControl::Delete {
                        object_id: item.0,
                        instance_sequence: owner(items, actor.as_deref(), binding.actor, *item)?
                            .current(SequenceKind::ObjectInstance, 0),
                    }
                    .encode(),
                ),
            };
            crate::session_output::push(&mut messages, &mut total, queue, bytes, limits)?;
        }
        for ((id, kind, property), count) in increments {
            for _ in 0..count {
                let sequences = if id == binding.actor {
                    actor.as_deref_mut().expect("validated actor counter owner")
                } else {
                    items.get_mut(&id).expect("validated item counter owner")
                };
                sequences
                    .advance(kind, property)
                    .expect("preflighted counter capacity");
            }
        }
        self.next = next;
        Ok(SessionBatch { binding, messages })
    }
}

fn property_kind(value: &bace_wire::PropertyValue<'_>) -> SequenceKind {
    match value {
        bace_wire::PropertyValue::Int(_) => SequenceKind::PropertyInt,
        bace_wire::PropertyValue::Int64(_) => SequenceKind::PropertyInt64,
        bace_wire::PropertyValue::Bool(_) => SequenceKind::PropertyBool,
        bace_wire::PropertyValue::Float(_) => SequenceKind::PropertyDouble,
        bace_wire::PropertyValue::String(_) => SequenceKind::PropertyString,
        bace_wire::PropertyValue::DataId(_) => SequenceKind::PropertyDataId,
        bace_wire::PropertyValue::InstanceId(_) => SequenceKind::PropertyInstanceId,
    }
}

fn owner<'a>(
    items: &'a BTreeMap<EntityId, Sequences>,
    actor: Option<&'a Sequences>,
    actor_id: EntityId,
    id: EntityId,
) -> Result<&'a Sequences, SessionProjectionError> {
    if id == actor_id {
        actor
    } else {
        items.get(&id)
    }
    .ok_or(SessionProjectionError::InvalidProjection)
}

fn social_limits(limits: BatchLimits) -> bace_wire::SocialCodecLimits {
    bace_wire::SocialCodecLimits {
        max_message_bytes: limits.max_message_bytes,
        max_string_bytes: limits.max_string_bytes,
        max_entries: 4096,
        max_filters: 32,
    }
}
