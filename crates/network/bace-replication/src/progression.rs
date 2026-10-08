//! Primary progression messages from immutable committed domain projections.
//! Player_Xp.SpendXP queues AvailableExperience before the owning trait update.
//! Rank-up sound/chat and Endurance's full Health follow-up use frozen
//! simulation-owned values. Optional run-rate effects remain separate.
use crate::{
    BatchLimits, ReplicationError, ReplicationMessage, SequenceKind, Sequences,
    SessionProjectionError,
};
use bace_gameplay_api::{
    ActionContext, CharacterBinding, ProgressionChange, ProgressionTarget, SkillAdvancement,
    TraitDetails,
};
use bace_wire::{AttributeUpdate, PropertyUpdate, PropertyValue, SkillUpdate, VitalUpdate};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankEffectPackets {
    pub owner: Vec<ReplicationMessage>,
    pub observers: Vec<ReplicationMessage>,
}

/// Source Player_{Attributes,Vitals,Skills}.HandleActionRaise* sends the
/// private trait update, then an optional public max-rank script, a private
/// RaiseTrait sound and Advancement chat. The accepted base is frozen by the
/// simulation; this projector never guesses derived skill/vital arithmetic.
pub fn project_rank_effect(
    actor: u32,
    change: ProgressionChange,
    limits: BatchLimits,
) -> Result<Option<RankEffectPackets>, SessionProjectionError> {
    use SessionProjectionError as E;
    let Some(effect) = change.rank_effect else {
        return Ok(None);
    };
    if actor == 0
        || change.before.target != change.after.target
        || change.before.ranks == change.after.ranks
    {
        return Err(E::InvalidProjection);
    }
    let name = match change.after.target {
        ProgressionTarget::Attribute(id) => match id {
            bace_gameplay_api::AttributeId::Strength => "Strength",
            bace_gameplay_api::AttributeId::Endurance => "Endurance",
            bace_gameplay_api::AttributeId::Quickness => "Quickness",
            bace_gameplay_api::AttributeId::Coordination => "Coordination",
            bace_gameplay_api::AttributeId::Focus => "Focus",
            bace_gameplay_api::AttributeId::SelfAttribute => "Self",
        },
        ProgressionTarget::Vital(id) => match id {
            bace_gameplay_api::VitalId::MaxHealth => "Maximum Health",
            bace_gameplay_api::VitalId::MaxStamina => "Maximum Stamina",
            bace_gameplay_api::VitalId::MaxMana => "Maximum Mana",
        },
        ProgressionTarget::Skill(id) => crate::training_notice::NAMES
            .get(id as usize)
            .ok_or(E::InvalidProjection)?,
    };
    let suffix = if effect.reached_maximum {
        " and has reached its upper limit"
    } else {
        ""
    };
    let text = if matches!(change.after.target, ProgressionTarget::Skill(_)) {
        format!("Your base {name} skill is now {}{suffix}!", effect.base)
    } else {
        format!("Your base {name} is now {}{suffix}!", effect.base)
    };
    if text.len() > limits.max_string_bytes {
        return Err(E::Limit);
    }
    let mut owner = Vec::with_capacity(if effect.reached_maximum { 3 } else { 2 });
    let mut observers = Vec::new();
    if effect.reached_maximum {
        let script = bace_wire::CombatEffect::Script {
            object_id: actor,
            script_id: 0x8d,
            speed: 1.0,
        }
        .encode(limits.max_string_bytes, limits.max_message_bytes)?;
        owner.push(ReplicationMessage {
            queue: 10,
            bytes: script.clone(),
        });
        observers.push(ReplicationMessage {
            queue: 10,
            bytes: script,
        });
    }
    owner.push(ReplicationMessage {
        queue: 10,
        bytes: bace_wire::CombatEffect::Sound {
            object_id: actor,
            sound_id: 0x8b,
            volume: 1.0,
        }
        .encode(limits.max_string_bytes, limits.max_message_bytes)?,
    });
    owner.push(ReplicationMessage {
        queue: 9,
        bytes: bace_wire::ChatMessage::System {
            text: &text,
            chat_type: 13,
        }
        .encode()?,
    });
    let bytes = owner
        .iter()
        .chain(&observers)
        .map(|m| m.bytes.len())
        .sum::<usize>();
    if owner.len() > limits.max_messages
        || bytes > limits.max_bytes
        || owner
            .iter()
            .any(|m| m.bytes.len() > limits.max_message_bytes)
    {
        return Err(E::Limit);
    }
    Ok(Some(RankEffectPackets { owner, observers }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionProjectionError {
    WrongBinding,
    StaleRevision,
    StaleSequence,
    MissingDetails,
    InvalidProjection,
    Capacity,
}
/// The caller owns this entire ordered batch until reliable admission succeeds.
/// A failure must retain it or close the peer; it is not a persistence receipt.
pub struct ProgressionPackets {
    pub context: ActionContext,
    pub revision: u64,
    pub queue: u16,
    pub messages: [Vec<u8>; 2],
    /// Full Health update after Endurance's sound/chat. Its Vital sequence is
    /// reserved atomically with the two primary property sequences.
    pub follow_up_vital: Option<Vec<u8>>,
    pub rank_changed: bool,
}
/// One character/session's primary output sequence state. No mutable gameplay
/// aggregate or physical state is duplicated here.
pub struct ProgressionCursor {
    binding: CharacterBinding,
    last_revision: u64,
    last_sequence: Option<u32>,
}
impl ProgressionCursor {
    pub fn new(binding: CharacterBinding, last_revision: u64) -> Self {
        Self {
            binding,
            last_revision,
            last_sequence: None,
        }
    }
    pub fn project(
        &mut self,
        sequences: &mut Sequences,
        context: ActionContext,
        change: ProgressionChange,
    ) -> Result<ProgressionPackets, ProgressionProjectionError> {
        if context.session != self.binding.session
            || context.account != self.binding.account
            || context.actor != self.binding.actor
        {
            return Err(ProgressionProjectionError::WrongBinding);
        }
        if change.revision < self.last_revision
            || (change.revision == self.last_revision && change.before != change.after)
        {
            return Err(ProgressionProjectionError::StaleRevision);
        }
        if self.last_sequence.is_some_and(|last| {
            let delta = context.sequence.wrapping_sub(last);
            delta == 0 || delta >= 0x8000_0000
        }) {
            return Err(ProgressionProjectionError::StaleSequence);
        }
        if change.before.target != change.after.target
            || change.available_experience > i64::MAX as u64
        {
            return Err(ProgressionProjectionError::InvalidProjection);
        }
        let details = change
            .after
            .details
            .ok_or(ProgressionProjectionError::MissingDetails)?;
        let (kind, property) = match (change.after.target, details) {
            (ProgressionTarget::Attribute(id), TraitDetails::Attribute { .. }) => {
                (SequenceKind::Attribute, id as u32)
            }
            (ProgressionTarget::Vital(id), TraitDetails::Vital { .. }) => {
                (SequenceKind::Vital, id as u32)
            }
            (ProgressionTarget::Skill(id), TraitDetails::Skill { last_used_time, .. })
                if last_used_time.is_finite()
                    && matches!(
                        change.after.advancement,
                        SkillAdvancement::Trained | SkillAdvancement::Specialized
                    ) =>
            {
                (SequenceKind::Skill, id)
            }
            _ => return Err(ProgressionProjectionError::InvalidProjection),
        };
        let follow_up = match change.follow_up_vital {
            Some(projection)
                if change.after.target
                    == ProgressionTarget::Attribute(bace_gameplay_api::AttributeId::Endurance)
                    && change.before.ranks != change.after.ranks
                    && projection.target
                        == ProgressionTarget::Vital(bace_gameplay_api::VitalId::MaxHealth)
                    && projection.advancement == SkillAdvancement::Inactive =>
            {
                let Some(TraitDetails::Vital {
                    starting_value,
                    current,
                }) = projection.details
                else {
                    return Err(ProgressionProjectionError::InvalidProjection);
                };
                Some((projection, starting_value, current))
            }
            Some(_) => return Err(ProgressionProjectionError::InvalidProjection),
            None => None,
        };
        let (xp_sequence, trait_sequence, vital_sequence) = if follow_up.is_some() {
            let [xp, trait_update, vital] = sequences
                .advance_batch([
                    (SequenceKind::PropertyInt64, 2),
                    (kind, property),
                    (SequenceKind::Vital, 1),
                ])
                .map_err(|_| ProgressionProjectionError::Capacity)?;
            (xp, trait_update, Some(vital))
        } else {
            let [xp, trait_update] = sequences
                .advance_batch([(SequenceKind::PropertyInt64, 2), (kind, property)])
                .map_err(|_| ProgressionProjectionError::Capacity)?;
            (xp, trait_update, None)
        };
        let xp = PropertyUpdate {
            sequence: xp_sequence as u8,
            object_id: None,
            property: 2,
            value: PropertyValue::Int64(change.available_experience as i64),
        }
        .encode()
        .expect("fixed int64 update");
        let after = change.after;
        let update = match details {
            TraitDetails::Attribute { starting_value } => AttributeUpdate {
                sequence: trait_sequence as u8,
                attribute: property,
                ranks: after.ranks.into(),
                starting_value,
                experience_spent: after.experience_spent,
            }
            .encode(),
            TraitDetails::Vital {
                starting_value,
                current,
            } => VitalUpdate {
                sequence: trait_sequence as u8,
                object_id: None,
                vital: property,
                ranks: after.ranks.into(),
                starting_value,
                current,
                experience_spent: after.experience_spent,
            }
            .encode(),
            TraitDetails::Skill {
                initial_level,
                resistance_at_last_check,
                last_used_time,
            } => SkillUpdate {
                sequence: trait_sequence as u8,
                skill: property,
                ranks: after.ranks,
                advancement_class: after.advancement as u32,
                experience_spent: after.experience_spent,
                initial_level,
                resistance_at_last_check,
                last_used_time,
            }
            .encode(),
        };
        self.last_revision = change.revision;
        self.last_sequence = Some(context.sequence);
        let follow_up_vital = follow_up.map(|(projection, starting_value, current)| {
            VitalUpdate {
                sequence: vital_sequence.expect("reserved vital sequence") as u8,
                object_id: None,
                vital: 1,
                ranks: projection.ranks.into(),
                starting_value,
                experience_spent: projection.experience_spent,
                current,
            }
            .encode()
        });
        Ok(ProgressionPackets {
            context,
            revision: change.revision,
            queue: 9,
            messages: [xp, update],
            follow_up_vital,
            rank_changed: change.before.ranks != change.after.ranks,
        })
    }
}

/// Ordered updates for an already durably committed skill transition.
/// Class-only training follows approved divergence #14; PP/init changes also
/// require a full private snapshot because the client's AC update retains them.
pub struct SkillPackets {
    pub context: ActionContext,
    pub revision: u64,
    pub queue: u16,
    pub messages: Vec<Vec<u8>>,
}
impl ProgressionCursor {
    pub fn project_training(
        &mut self,
        sequences: &mut Sequences,
        context: ActionContext,
        change: bace_gameplay_api::SkillTrainingChange,
        available_xp: Option<u64>,
    ) -> Result<SkillPackets, ProgressionProjectionError> {
        use ProgressionProjectionError as E;
        if context.session != self.binding.session
            || context.account != self.binding.account
            || context.actor != self.binding.actor
        {
            return Err(E::WrongBinding);
        }
        if change.revision <= self.last_revision {
            return Err(E::StaleRevision);
        }
        if self.last_sequence.is_some_and(|last| {
            let delta = context.sequence.wrapping_sub(last);
            delta == 0 || delta >= 0x80000000
        }) {
            return Err(E::StaleSequence);
        }
        let ProgressionTarget::Skill(skill) = change.after.target else {
            return Err(E::InvalidProjection);
        };
        if change.before.target != change.after.target
            || change.available_skill_credits > i32::MAX as u32
            || available_xp.is_some_and(|n| n > i64::MAX as u64)
        {
            return Err(E::InvalidProjection);
        }
        let Some(TraitDetails::Skill {
            initial_level,
            resistance_at_last_check,
            last_used_time,
        }) = change.after.details
        else {
            return Err(E::MissingDetails);
        };
        if !last_used_time.is_finite() || skill == 0 || skill > 54 || context.actor.0 == 0 {
            return Err(E::InvalidProjection);
        }
        if !matches!(
            change.after.advancement,
            SkillAdvancement::Untrained | SkillAdvancement::Trained | SkillAdvancement::Specialized
        ) {
            return Err(E::InvalidProjection);
        }
        let full = change.before.ranks != change.after.ranks
            || change.before.experience_spent != change.after.experience_spent
            || change.before.details != change.after.details;
        let (class_sequence, credit_sequence, xp_sequence) = if available_xp.is_some() {
            let [class, credits, xp] = sequences
                .advance_batch([
                    (SequenceKind::Skill, skill),
                    (SequenceKind::PropertyInt, 24),
                    (SequenceKind::PropertyInt64, 2),
                ])
                .map_err(|_| E::Capacity)?;
            (class, credits, xp)
        } else {
            let [class, credits] = sequences
                .advance_batch([
                    (SequenceKind::Skill, skill),
                    (SequenceKind::PropertyInt, 24),
                ])
                .map_err(|_| E::Capacity)?;
            (class, credits, 0)
        };
        let mut messages = Vec::with_capacity(4);
        messages.push(
            bace_wire::SkillClassUpdate {
                sequence: class_sequence as u8,
                object: context.actor.0,
                skill,
                advancement: change.after.advancement as u32,
            }
            .encode()
            .map_err(|_| E::InvalidProjection)?,
        );
        if full {
            let [sequence] = sequences
                .advance_batch([(SequenceKind::Skill, skill)])
                .map_err(|_| E::Capacity)?;
            messages.push(
                SkillUpdate {
                    sequence: sequence as u8,
                    skill,
                    ranks: change.after.ranks,
                    advancement_class: change.after.advancement as u32,
                    experience_spent: change.after.experience_spent,
                    initial_level,
                    resistance_at_last_check,
                    last_used_time,
                }
                .encode(),
            );
        }
        messages.push(
            PropertyUpdate {
                sequence: credit_sequence as u8,
                object_id: None,
                property: 24,
                value: PropertyValue::Int(change.available_skill_credits as i32),
            }
            .encode()
            .map_err(|_| E::InvalidProjection)?,
        );
        if let Some(xp) = available_xp {
            messages.push(
                PropertyUpdate {
                    sequence: xp_sequence as u8,
                    object_id: None,
                    property: 2,
                    value: PropertyValue::Int64(xp as i64),
                }
                .encode()
                .map_err(|_| E::InvalidProjection)?,
            );
        }
        self.last_revision = change.revision;
        self.last_sequence = Some(context.sequence);
        Ok(SkillPackets {
            context,
            revision: change.revision,
            queue: 9,
            messages,
        })
    }
}

/// Standalone compatibility adapter. Live runtimes use `ProgressionCursor`
/// with the existing session's canonical property counters.
pub struct ProgressionProjector {
    projection: ProgressionCursor,
    sequences: Sequences,
}
impl ProgressionProjector {
    pub fn new(
        binding: CharacterBinding,
        last_revision: u64,
        sequence_capacity: usize,
    ) -> Result<Self, ReplicationError> {
        Ok(Self {
            projection: ProgressionCursor::new(binding, last_revision),
            sequences: Sequences::new(sequence_capacity)?,
        })
    }
    pub fn project(
        &mut self,
        context: ActionContext,
        change: ProgressionChange,
    ) -> Result<ProgressionPackets, ProgressionProjectionError> {
        self.projection
            .project(&mut self.sequences, context, change)
    }
    pub fn project_training(
        &mut self,
        context: ActionContext,
        change: bace_gameplay_api::SkillTrainingChange,
        available_xp: Option<u64>,
    ) -> Result<SkillPackets, ProgressionProjectionError> {
        self.projection
            .project_training(&mut self.sequences, context, change, available_xp)
    }
}
