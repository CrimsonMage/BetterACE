//! Per-session ordered output. A batch is retained until reliable admission;
//! successful projection is neither network delivery nor durable gameplay success.
use bace_gameplay_api::CharacterBinding;
use bace_wire::{
    CharacterTitle, CombatEvent, ContainerEntry, FriendsUpdate, FriendsUpdateKind, InventoryEvent,
    ObjectCodecLimits, ObjectControl, ObjectDescription, PlayerDescription,
    PlayerDescriptionLimits, SocialCodecLimits, SocialEvent, WireError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicationMessage {
    pub queue: u16,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBatch {
    pub binding: CharacterBinding,
    pub messages: Vec<ReplicationMessage>,
}
#[derive(Clone, Copy, Debug)]
pub struct BatchLimits {
    pub max_messages: usize,
    pub max_bytes: usize,
    pub max_message_bytes: usize,
    pub max_string_bytes: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct LoginProjectionLimits {
    pub batch: BatchLimits,
    pub description: PlayerDescriptionLimits,
    pub objects: ObjectCodecLimits,
    pub social: SocialCodecLimits,
    pub max_titles: usize,
    pub max_container_items: usize,
}
#[derive(Clone, Copy, Debug)]
pub enum LoginPossession<'a> {
    Create(&'a ObjectDescription),
    Contents {
        container_id: u32,
        items: &'a [ContainerEntry],
    },
}
pub struct LoginProjection<'a> {
    pub description: &'a PlayerDescription,
    pub titles: &'a CharacterTitle,
    pub friends: &'a FriendsUpdate,
    pub self_object: &'a ObjectDescription,
    /// ACE main inventory order: item create, contained view/create messages,
    /// followed by equipped-object creates. The world owner supplies this order.
    pub possessions: &'a [LoginPossession<'a>],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionProjectionError {
    WrongBinding,
    InvalidProjection,
    Limit,
    Wire(WireError),
}
impl From<WireError> for SessionProjectionError {
    fn from(value: WireError) -> Self {
        Self::Wire(value)
    }
}
/// One event counter for all event families for this authenticated generation.
/// All encoding and batch admission checks finish before advancing the counter.
pub struct EventSequencer {
    pub(crate) binding: CharacterBinding,
    pub(crate) next: u32,
}
impl EventSequencer {
    pub fn project_magic(
        &mut self,
        binding: CharacterBinding,
        events: &[bace_wire::MagicEvent<'_>],
        max_enchantments: usize,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if events.len() > limits.max_messages {
            return Err(SessionProjectionError::Limit);
        }
        let mut messages = Vec::with_capacity(events.len());
        let mut total = 0;
        for (index, event) in events.iter().enumerate() {
            let bytes = event.encode(
                binding.actor.0,
                self.next.wrapping_add(index as u32),
                max_enchantments,
                limits.max_message_bytes,
            )?;
            push(&mut messages, &mut total, 9, bytes, limits)?;
        }
        self.next = self.next.wrapping_add(events.len() as u32);
        Ok(SessionBatch { binding, messages })
    }
    pub fn new(binding: CharacterBinding, next: u32) -> Self {
        Self { binding, next }
    }
    pub fn next_sequence(&self) -> u32 {
        self.next
    }
    pub fn project_combat(
        &mut self,
        binding: CharacterBinding,
        events: &[CombatEvent<'_>],
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if events.len() > limits.max_messages {
            return Err(SessionProjectionError::Limit);
        }
        let mut messages = Vec::with_capacity(events.len());
        let mut total = 0;
        for (i, event) in events.iter().enumerate() {
            let bytes = event.encode(
                binding.actor.0,
                self.next.wrapping_add(i as u32),
                limits.max_string_bytes,
                limits.max_message_bytes,
            )?;
            push(&mut messages, &mut total, 9, bytes, limits)?;
        }
        self.next = self.next.wrapping_add(events.len() as u32);
        Ok(SessionBatch { binding, messages })
    }
    pub fn project_login(
        &mut self,
        binding: CharacterBinding,
        input: LoginProjection<'_>,
        limits: LoginProjectionLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        if input.self_object.object_id != binding.actor.0
            || input.friends.kind != FriendsUpdateKind::Full
        {
            return Err(SessionProjectionError::InvalidProjection);
        }
        let count = input
            .possessions
            .len()
            .checked_add(5)
            .ok_or(SessionProjectionError::Limit)?;
        if count > limits.batch.max_messages {
            return Err(SessionProjectionError::Limit);
        }
        let mut messages = Vec::with_capacity(count);
        let mut total = 0;
        let mut sequence = self.next;
        let description =
            input
                .description
                .encode(binding.actor.0, sequence, limits.description)?;
        sequence = sequence.wrapping_add(1);
        push(&mut messages, &mut total, 9, description, limits.batch)?;
        let titles = input.titles.encode(
            binding.actor.0,
            sequence,
            limits.max_titles,
            limits.batch.max_message_bytes,
        )?;
        sequence = sequence.wrapping_add(1);
        push(&mut messages, &mut total, 9, titles, limits.batch)?;
        let friends =
            SocialEvent::Friends(input.friends).encode(binding.actor.0, sequence, limits.social)?;
        sequence = sequence.wrapping_add(1);
        push(&mut messages, &mut total, 9, friends, limits.batch)?;
        push(
            &mut messages,
            &mut total,
            10,
            ObjectControl::PlayerCreate {
                object_id: binding.actor.0,
            }
            .encode(),
            limits.batch,
        )?;
        push(
            &mut messages,
            &mut total,
            10,
            input.self_object.encode_create(limits.objects)?,
            limits.batch,
        )?;
        for possession in input.possessions {
            match possession {
                LoginPossession::Create(object) => push(
                    &mut messages,
                    &mut total,
                    10,
                    object.encode_create(limits.objects)?,
                    limits.batch,
                )?,
                LoginPossession::Contents {
                    container_id,
                    items,
                } => {
                    let bytes = InventoryEvent::ViewContents {
                        container_id: *container_id,
                        items,
                    }
                    .encode(
                        binding.actor.0,
                        sequence,
                        limits.max_container_items,
                        limits.batch.max_message_bytes,
                    )?;
                    sequence = sequence.wrapping_add(1);
                    push(&mut messages, &mut total, 9, bytes, limits.batch)?;
                }
            }
        }
        self.next = sequence;
        Ok(SessionBatch { binding, messages })
    }
    pub(crate) fn check_binding(
        &self,
        binding: CharacterBinding,
    ) -> Result<(), SessionProjectionError> {
        if binding == self.binding {
            Ok(())
        } else {
            Err(SessionProjectionError::WrongBinding)
        }
    }
}
pub(crate) fn push(
    messages: &mut Vec<ReplicationMessage>,
    total: &mut usize,
    queue: u16,
    bytes: Vec<u8>,
    limits: BatchLimits,
) -> Result<(), SessionProjectionError> {
    let size = total
        .checked_add(bytes.len())
        .ok_or(SessionProjectionError::Limit)?;
    if messages.len() >= limits.max_messages
        || size > limits.max_bytes
        || bytes.len() > limits.max_message_bytes
    {
        return Err(SessionProjectionError::Limit);
    }
    messages.push(ReplicationMessage { queue, bytes });
    *total = size;
    Ok(())
}
