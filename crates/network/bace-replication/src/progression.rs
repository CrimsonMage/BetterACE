//! Primary progression messages from immutable committed domain projections.
//! Player_Xp.SpendXP queues AvailableExperience before the owning trait update.
//! Rank-up sounds/chat, derived vitals and optional run-rate effects require
//! separate authoritative effects and are not fabricated by this projector.
use crate::{ReplicationError, SequenceKind, Sequences};
use bace_gameplay_api::{
    ActionContext, CharacterBinding, ProgressionChange, ProgressionTarget, SkillAdvancement,
    TraitDetails,
};
use bace_wire::{AttributeUpdate, PropertyUpdate, PropertyValue, SkillUpdate, VitalUpdate};

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
    pub rank_changed: bool,
}
/// One character/session's primary output sequence state. No mutable gameplay
/// aggregate or physical state is duplicated here.
pub struct ProgressionProjector {
    binding: CharacterBinding,
    sequences: Sequences,
    last_revision: u64,
    last_sequence: Option<u32>,
}
impl ProgressionProjector {
    pub fn new(
        binding: CharacterBinding,
        last_revision: u64,
        sequence_capacity: usize,
    ) -> Result<Self, ReplicationError> {
        Ok(Self {
            binding,
            sequences: Sequences::new(sequence_capacity)?,
            last_revision,
            last_sequence: None,
        })
    }
    pub fn project(
        &mut self,
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
        let [xp_sequence, trait_sequence] = self
            .sequences
            .advance_batch([(SequenceKind::PropertyInt64, 2), (kind, property)])
            .map_err(|_| ProgressionProjectionError::Capacity)?;
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
        Ok(ProgressionPackets {
            context,
            revision: change.revision,
            queue: 9,
            messages: [xp, update],
            rank_changed: change.before.ranks != change.after.ranks,
        })
    }
}
