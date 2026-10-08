//! Explicit frozen V3 ↔ authoritative progression bridge. No SQL or clocks.
//! Prepared tables must already have passed DAT fingerprint/version admission.
use crate::character_assets::PreparedCharacterAssets;
use bace_character::{
    CharacterProgression, LuminanceState, SkillCosts, SkillProposal, SkillTrainingRules,
    TraitProgress, TraitState,
};
use bace_content::{Attribute, Property, SecondaryAttribute, Skill};
use bace_gameplay_api::{AttributeId, ProgressionTarget, SkillAdvancement, TraitDetails, VitalId};
use bace_storage_codec::PlayerSaveV6;
use std::sync::Arc;

const AUGMENTATIONS: [(u32, u32); 5] = [(40, 224), (18, 225), (29, 226), (30, 227), (28, 228)];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionSaveError {
    InvalidSave,
    MissingProperty(u32),
    InvalidProperty(u32),
    UnknownHeritage,
    InvalidRules,
    InvalidProgression,
    RankMismatch,
    MissingDetails,
    StaleState,
}

pub fn restore_progression(
    saved: &PlayerSaveV6,
    assets: &PreparedCharacterAssets,
) -> Result<CharacterProgression, ProgressionSaveError> {
    saved
        .validate()
        .map_err(|_| ProgressionSaveError::InvalidSave)?;
    let p = &saved.player.entity.state.properties;
    let int = |id| {
        p.ints
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.value)
            .ok_or(ProgressionSaveError::MissingProperty(id))
    };
    let xp = p
        .int64s
        .iter()
        .find(|v| v.id == 2)
        .map(|v| v.value)
        .ok_or(ProgressionSaveError::MissingProperty(2))?;
    let credits = int(24)?;
    let heritage = int(188)?;
    if xp < 0 {
        return Err(ProgressionSaveError::InvalidProperty(2));
    }
    if credits < 0 {
        return Err(ProgressionSaveError::InvalidProperty(24));
    }
    let heritage = assets
        .char_gen()
        .heritage_groups
        .get(&(heritage as u32))
        .ok_or(ProgressionSaveError::UnknownHeritage)?;
    let mut costs = Vec::with_capacity(assets.skill_table().skills.len());
    for (&skill, def) in &assets.skill_table().skills {
        let upgrade = def
            .specialized_cost
            .checked_sub(def.trained_cost)
            .ok_or(ProgressionSaveError::InvalidRules)?;
        costs.push(SkillCosts {
            skill,
            trained_cost: def.trained_cost,
            specialized_cost: upgrade,
        });
    }
    let overrides: Vec<_> = heritage
        .skills
        .iter()
        .map(|s| (s.skill, s.primary_cost as u32))
        .collect();
    let rules = SkillTrainingRules::new(&costs)
        .and_then(|r| r.with_specialization_costs(&overrides))
        .map_err(|_| ProgressionSaveError::InvalidRules)?;
    let mut states =
        Vec::with_capacity(p.attributes.len() + p.secondary_attributes.len() + p.skills.len());
    let mut ranks = Vec::with_capacity(states.capacity());
    for item in &p.attributes {
        let Ok(id) = AttributeId::try_from(item.id) else {
            continue;
        };
        states.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(id),
                experience_spent: item.value.cp_spent,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute {
                starting_value: item.value.init_level,
            },
        });
        ranks.push(item.value.level_from_cp);
    }
    for item in &p.secondary_attributes {
        let Ok(id) = VitalId::try_from(item.id) else {
            continue;
        };
        states.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Vital(id),
                experience_spent: item.value.cp_spent,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Vital {
                starting_value: item.value.init_level,
                current: item.value.current_level,
            },
        });
        ranks.push(item.value.level_from_cp);
    }
    for item in &p.skills {
        let skill = u32::try_from(item.id).map_err(|_| ProgressionSaveError::InvalidProgression)?;
        if !assets.skill_table().skills.contains_key(&skill) {
            continue;
        }
        states.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(skill),
                experience_spent: item.value.pp,
                advancement: SkillAdvancement::try_from(item.value.sac)
                    .map_err(|_| ProgressionSaveError::InvalidProgression)?,
            },
            details: TraitDetails::Skill {
                initial_level: item.value.init_level,
                resistance_at_last_check: item.value.resistance_at_last_check,
                last_used_time: item.value.last_used_time,
            },
        });
        ranks.push(u32::from(item.value.level_from_pp));
    }
    let mut state = CharacterProgression::with_state(
        &states,
        assets.progression(),
        xp as u64,
        saved.player.entity.mutation_revision,
    )
    .map_err(|_| ProgressionSaveError::InvalidProgression)?;
    for (entry, rank) in states.iter().zip(ranks) {
        if state
            .projection(entry.progress.target)
            .is_none_or(|v| u32::from(v.ranks) != rank)
        {
            return Err(ProgressionSaveError::RankMismatch);
        }
    }
    let mut augs = Vec::new();
    for (skill, property) in AUGMENTATIONS {
        let value = p
            .ints
            .iter()
            .find(|v| v.id == property)
            .map_or(0, |v| v.value);
        if !(0..=1).contains(&value) {
            return Err(ProgressionSaveError::InvalidProperty(property));
        }
        if value == 1 {
            augs.push(skill);
        }
    }
    state = state
        .with_training(Arc::new(rules), credits as u32, &augs)
        .map_err(|_| ProgressionSaveError::InvalidProgression)?;
    let available = p.int64s.iter().find(|v| v.id == 6).map(|v| v.value);
    let maximum = p.int64s.iter().find(|v| v.id == 7).map(|v| v.value);
    match (available, maximum) {
        (None, None) => {}
        (Some(available), Some(maximum)) => {
            state = state
                .with_luminance(LuminanceState { available, maximum })
                .map_err(|_| ProgressionSaveError::InvalidProgression)?;
        }
        _ => return Err(ProgressionSaveError::InvalidProperty(6)),
    }
    Ok(state)
}
/// Compare the complete before-projection, then materialize only the proposed
/// changed fields. The caller adds device consumption to the same transaction.
pub fn freeze_skill_proposal(
    saved: &PlayerSaveV6,
    assets: &PreparedCharacterAssets,
    proposal: &SkillProposal,
) -> Result<PlayerSaveV6, ProgressionSaveError> {
    let loaded = restore_progression(saved, assets)?;
    let expected = proposal.expected_state();
    if loaded.revision() != expected.revision()
        || loaded.available_experience() != expected.available_experience()
        || loaded.available_skill_credits() != expected.available_skill_credits()
        || !loaded.trait_states().eq(expected.trait_states())
        || !loaded.augmented_skills().eq(expected.augmented_skills())
        || loaded.luminance() != expected.luminance()
    {
        return Err(ProgressionSaveError::StaleState);
    }
    freeze_progression(saved, proposal.proposed_state())
}
/// Routine owner snapshot, retaining all unrelated numeric properties and V3
/// UI/rare/enchantment/quest metadata. Caller supplies current auxiliary state.
pub fn freeze_progression(
    saved: &PlayerSaveV6,
    state: &CharacterProgression,
) -> Result<PlayerSaveV6, ProgressionSaveError> {
    saved
        .validate()
        .map_err(|_| ProgressionSaveError::InvalidSave)?;
    if state.revision() < saved.player.entity.mutation_revision {
        return Err(ProgressionSaveError::StaleState);
    }
    let mut next = saved.clone();
    let p = &mut next.player.entity.state.properties;
    put(&mut p.int64s, 2, state.available_experience() as i64);
    put(
        &mut p.ints,
        24,
        state
            .available_skill_credits()
            .ok_or(ProgressionSaveError::MissingProperty(24))? as i32,
    );
    for (trait_, details) in state.trait_states() {
        let projection = state
            .projection(trait_.target)
            .ok_or(ProgressionSaveError::InvalidProgression)?;
        match (trait_.target, details) {
            (
                ProgressionTarget::Attribute(id),
                Some(TraitDetails::Attribute { starting_value }),
            ) => put(
                &mut p.attributes,
                id as u32,
                Attribute {
                    init_level: starting_value,
                    level_from_cp: u32::from(projection.ranks),
                    cp_spent: trait_.experience_spent,
                },
            ),
            (
                ProgressionTarget::Vital(id),
                Some(TraitDetails::Vital {
                    starting_value,
                    current,
                }),
            ) => put(
                &mut p.secondary_attributes,
                id as u32,
                SecondaryAttribute {
                    init_level: starting_value,
                    level_from_cp: u32::from(projection.ranks),
                    cp_spent: trait_.experience_spent,
                    current_level: current,
                },
            ),
            (
                ProgressionTarget::Skill(id),
                Some(TraitDetails::Skill {
                    initial_level,
                    resistance_at_last_check,
                    last_used_time,
                }),
            ) => {
                let id = i32::try_from(id).map_err(|_| ProgressionSaveError::InvalidProgression)?;
                let value = Skill {
                    level_from_pp: projection.ranks,
                    sac: trait_.advancement as u32,
                    pp: trait_.experience_spent,
                    init_level: initial_level,
                    resistance_at_last_check,
                    last_used_time,
                };
                if let Some(item) = p.skills.iter_mut().find(|v| v.id == id) {
                    item.value = value;
                } else {
                    p.skills.push(Property { id, value });
                }
            }
            _ => return Err(ProgressionSaveError::MissingDetails),
        }
    }
    for (skill, property) in AUGMENTATIONS {
        if state.augmented_skills().any(|s| s == skill) {
            put(&mut p.ints, property, 1);
        } else if p.ints.iter().any(|v| v.id == property && v.value != 0) {
            return Err(ProgressionSaveError::StaleState);
        }
    }
    if let Some(lum) = state.luminance() {
        put(&mut p.int64s, 6, lum.available);
        put(&mut p.int64s, 7, lum.maximum);
    }
    next.player.entity.mutation_revision = state.revision();
    next.validate()
        .map_err(|_| ProgressionSaveError::InvalidSave)?;
    Ok(next)
}
fn put<T>(values: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(item) = values.iter_mut().find(|v| v.id == id) {
        item.value = value;
    } else {
        values.push(Property { id, value });
    }
}

/// Freeze an authenticated simulation ticket returned by the bounded owner
/// channel. Revision and complete before-skill data fence the immutable change;
/// the repository still checks the whole aggregate CAS version and operation ID.
pub fn freeze_skill_change(
    saved: &PlayerSaveV6,
    change: bace_character::SkillTransitionChange,
    expected_revision: u64,
) -> Result<PlayerSaveV6, ProgressionSaveError> {
    saved
        .validate()
        .map_err(|_| ProgressionSaveError::InvalidSave)?;
    if saved.player.entity.mutation_revision != expected_revision
        || expected_revision.checked_add(1) != Some(change.revision)
        || change.before.target != change.after.target
        || change.available_experience > i64::MAX as u64
        || change.available_skill_credits > i32::MAX as u32
    {
        return Err(ProgressionSaveError::StaleState);
    }
    let ProgressionTarget::Skill(skill) = change.before.target else {
        return Err(ProgressionSaveError::InvalidProgression);
    };
    let id = i32::try_from(skill).map_err(|_| ProgressionSaveError::InvalidProgression)?;
    let before = saved
        .player
        .entity
        .state
        .properties
        .skills
        .iter()
        .find(|v| v.id == id);
    let expected = match change.before.details {
        Some(TraitDetails::Skill {
            initial_level,
            resistance_at_last_check,
            last_used_time,
        }) => Skill {
            level_from_pp: change.before.ranks,
            sac: change.before.advancement as u32,
            pp: change.before.experience_spent,
            init_level: initial_level,
            resistance_at_last_check,
            last_used_time,
        },
        _ => return Err(ProgressionSaveError::MissingDetails),
    };
    if let Some(before) = before {
        if before.value != expected {
            return Err(ProgressionSaveError::StaleState);
        }
    } else if expected
        != (Skill {
            sac: 1,
            ..Default::default()
        })
    {
        return Err(ProgressionSaveError::StaleState);
    }
    let Some(TraitDetails::Skill {
        initial_level,
        resistance_at_last_check,
        last_used_time,
    }) = change.after.details
    else {
        return Err(ProgressionSaveError::MissingDetails);
    };
    let after = Skill {
        level_from_pp: change.after.ranks,
        sac: change.after.advancement as u32,
        pp: change.after.experience_spent,
        init_level: initial_level,
        resistance_at_last_check,
        last_used_time,
    };
    let mut next = saved.clone();
    let p = &mut next.player.entity.state.properties;
    if let Some(record) = p.skills.iter_mut().find(|v| v.id == id) {
        record.value = after;
    } else {
        p.skills.push(Property { id, value: after });
    }
    put(&mut p.int64s, 2, change.available_experience as i64);
    put(&mut p.ints, 24, change.available_skill_credits as i32);
    if change.augmentation_added {
        let property = AUGMENTATIONS
            .iter()
            .find(|(id, _)| *id == skill)
            .ok_or(ProgressionSaveError::InvalidProgression)?
            .1;
        if p.ints.iter().any(|v| v.id == property && v.value != 0) {
            return Err(ProgressionSaveError::StaleState);
        }
        put(&mut p.ints, property, 1);
    }
    next.player.entity.mutation_revision = change.revision;
    next.validate()
        .map_err(|_| ProgressionSaveError::InvalidSave)?;
    Ok(next)
}
