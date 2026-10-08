//! Freeze one already-prepared character effect, never an entire conversation.
use crate::{game_inventory::set, native_player};
use bace_content::Property;
use bace_entity::{PropertyFamily as F, PropertyValue as V};
use bace_simulation::NpcEffect;
use bace_storage_codec::{ContractSaveV1, PlayerSaveV6, QuestSaveV1, SaveCodecError};

const CONFLICT: SaveCodecError =
    SaveCodecError::Invalid("NPC player effect identity or before-state conflict");

pub fn freeze_player_effect(
    saved: &PlayerSaveV6,
    effect: &NpcEffect,
) -> Result<PlayerSaveV6, SaveCodecError> {
    saved.validate()?;
    let mut next = saved.clone();
    let (actor, before, after) = match effect {
        NpcEffect::Property {
            actor,
            aggregate: Some(fence),
            change,
        } => {
            let props = &mut next.player.entity.state.properties;
            match change.family {
                F::Bool => property(
                    &mut props.bools,
                    change.stat,
                    &change.before,
                    &change.after,
                    V::Bool,
                    |v| if let V::Bool(v) = v { Some(*v) } else { None },
                )?,
                F::Int => property(
                    &mut props.ints,
                    change.stat,
                    &change.before,
                    &change.after,
                    V::Int,
                    |v| if let V::Int(v) = v { Some(*v) } else { None },
                )?,
                F::Int64 => property(
                    &mut props.int64s,
                    change.stat,
                    &change.before,
                    &change.after,
                    V::Int64,
                    |v| if let V::Int64(v) = v { Some(*v) } else { None },
                )?,
                F::Float => property(
                    &mut props.floats,
                    change.stat,
                    &change.before,
                    &change.after,
                    V::Float,
                    |v| if let V::Float(v) = v { Some(*v) } else { None },
                )?,
                F::String => property(
                    &mut props.strings,
                    change.stat,
                    &change.before,
                    &change.after,
                    V::String,
                    |v| {
                        if let V::String(v) = v {
                            Some(v.clone())
                        } else {
                            None
                        }
                    },
                )?,
                _ => {
                    return Err(SaveCodecError::Invalid(
                        "NPC trait effect requires its character owner adapter",
                    ));
                }
            }
            (*actor, fence.before_revision, fence.after_revision)
        }
        NpcEffect::Quest {
            actor,
            aggregate: Some(fence),
            change,
        } => {
            let key = bace_quests::quest_key(&change.name).map_err(|_| CONFLICT)?;
            let mut index = None;
            for (i, quest) in next.player.quests.iter().enumerate() {
                if bace_quests::quest_key(&quest.name).map_err(|_| CONFLICT)? == key
                    && index.replace(i).is_some()
                {
                    return Err(CONFLICT);
                }
            }
            let previous = index
                .map(|i| {
                    let quest = &next.player.quests[i];
                    Ok::<_, SaveCodecError>(bace_quests::QuestProgress {
                        completions: i32::from_ne_bytes(quest.completions.to_ne_bytes()),
                        last_completed_seconds: u32::try_from(quest.last_completed)
                            .map_err(|_| CONFLICT)?,
                    })
                })
                .transpose()?;
            if previous != change.before {
                return Err(CONFLICT);
            }
            if change.before != change.after {
                match (index, change.after) {
                    (Some(i), Some(value)) => {
                        next.player.quests[i].completions =
                            u32::from_ne_bytes(value.completions.to_ne_bytes());
                        next.player.quests[i].last_completed =
                            i64::from(value.last_completed_seconds);
                    }
                    (Some(i), None) => {
                        next.player.quests.remove(i);
                    }
                    (None, Some(value)) => next.player.quests.push(QuestSaveV1 {
                        name: change.name.clone(),
                        completions: u32::from_ne_bytes(value.completions.to_ne_bytes()),
                        last_completed: i64::from(value.last_completed_seconds),
                    }),
                    (None, None) => (),
                }
            }
            (*actor, fence.before_revision, fence.after_revision)
        }
        NpcEffect::Experience { actor, credit } => {
            experience(&mut next, credit)?;
            (*actor, credit.before_revision, credit.after_revision)
        }
        NpcEffect::Luminance { actor, credit } => {
            let props = &mut next.player.entity.state.properties;
            if scalar(&props.int64s, 6) != Some(credit.before.available)
                || scalar(&props.int64s, 7) != Some(credit.before.maximum)
            {
                return Err(CONFLICT);
            }
            if credit.after.available < 0
                || credit.after.maximum < 0
                || credit.after.available > credit.after.maximum
            {
                return Err(CONFLICT);
            }
            set(&mut props.int64s, 6, credit.after.available);
            set(&mut props.int64s, 7, credit.after.maximum);
            (*actor, credit.before_revision, credit.after_revision)
        }
        NpcEffect::CharacterService { actor, change } => {
            services(&mut next, change)?;
            (*actor, change.before_revision, change.after_revision)
        }
        NpcEffect::Contract {
            actor,
            before_revision,
            change,
        } => {
            let frozen = |v: &bace_quests::ContractState| ContractSaveV1 {
                id: v.id,
                display: v.display,
            };
            if next.contracts != change.before.iter().map(frozen).collect::<Vec<_>>() {
                return Err(CONFLICT);
            }
            next.contracts = change.after.iter().map(frozen).collect();
            let after = before_revision
                .checked_add(u64::from(change.before != change.after))
                .ok_or(CONFLICT)?;
            (*actor, *before_revision, after)
        }
        NpcEffect::EarnedExperience { actor, change } => {
            if change.services.before_revision != change.experience.before_revision
                || change.services.after_revision != change.experience.after_revision
            {
                return Err(CONFLICT);
            }
            experience(&mut next, &change.experience)?;
            services(&mut next, &change.services)?;
            let props = &mut next.player.entity.state.properties;
            let before = scalar(&props.ints, 24)
                .map(u32::try_from)
                .transpose()
                .map_err(|_| CONFLICT)?;
            if before != change.before_skill_credits {
                return Err(CONFLICT);
            }
            match change.after_skill_credits {
                Some(v) => set(&mut props.ints, 24, i32::try_from(v).map_err(|_| CONFLICT)?),
                None => props.ints.retain(|p| p.id != 24),
            }
            (
                *actor,
                change.experience.before_revision,
                change.experience.after_revision,
            )
        }
        NpcEffect::QueuedExperience { .. }
        | NpcEffect::Service(_)
        | NpcEffect::FellowQuest { .. }
        | NpcEffect::Property {
            aggregate: None, ..
        }
        | NpcEffect::Quest {
            aggregate: None, ..
        } => {
            return Err(SaveCodecError::Invalid(
                "NPC effect requires a different owner save adapter",
            ));
        }
    };
    if actor.0 != saved.player.entity.object_id
        || before != saved.player.entity.mutation_revision
        || after.checked_sub(before).is_none_or(|delta| delta > 1)
    {
        return Err(CONFLICT);
    }
    if after == before && next != *saved {
        return Err(CONFLICT);
    }
    next.player.entity.mutation_revision = after;
    next.validate()?;
    Ok(next)
}
fn scalar<T: Copy>(properties: &[Property<T>], id: u32) -> Option<T> {
    properties.iter().find(|p| p.id == id).map(|p| p.value)
}
fn experience(
    saved: &mut PlayerSaveV6,
    credit: &bace_character::ExperienceCredit,
) -> Result<(), SaveCodecError> {
    let before = scalar(&saved.player.entity.state.properties.int64s, 2).ok_or(CONFLICT)?;
    if u64::try_from(before).ok() != Some(credit.before_available) {
        return Err(CONFLICT);
    }
    set(
        &mut saved.player.entity.state.properties.int64s,
        2,
        i64::try_from(credit.after_available).map_err(|_| CONFLICT)?,
    );
    Ok(())
}
fn services(
    saved: &mut PlayerSaveV6,
    change: &bace_character::CharacterServiceChange,
) -> Result<(), SaveCodecError> {
    if native_player::restore_services(saved)? != change.before {
        return Err(CONFLICT);
    }
    let contracts = native_player::restore_contracts(saved)?;
    native_player::freeze_services(saved, &change.after, &contracts)
}
fn property<T: Clone>(
    properties: &mut Vec<Property<T>>,
    id: u32,
    before: &Option<V>,
    after: &Option<V>,
    wrap: impl Fn(T) -> V,
    unwrap: impl Fn(&V) -> Option<T>,
) -> Result<(), SaveCodecError> {
    if properties
        .iter()
        .find(|p| p.id == id)
        .map(|p| wrap(p.value.clone()))
        != *before
    {
        return Err(CONFLICT);
    }
    match after {
        Some(value) => set(properties, id, unwrap(value).ok_or(CONFLICT)?),
        None => properties.retain(|p| p.id != id),
    }
    Ok(())
}
