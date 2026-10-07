use std::sync::Arc;

use bace_character::{
    CharacterProgression, ProgressionStateError, ProgressionTables, RankTable, TraitProgress,
    TraitState,
};
use bace_gameplay_api::{
    AttributeId, ProgressionTarget, RaiseProgression, SkillAdvancement, TraitDetails, VitalId,
};

fn tables() -> Arc<ProgressionTables> {
    let table = RankTable::new(&[0, 1, 10, 100]).unwrap();
    Arc::new(ProgressionTables {
        attributes: table.clone(),
        vitals: table.clone(),
        trained_skills: table.clone(),
        specialized_skills: table,
    })
}

fn states() -> [TraitState; 3] {
    [
        TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(AttributeId::Strength),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute { starting_value: 75 },
        },
        TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Vital(VitalId::MaxHealth),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Vital {
                starting_value: 12,
                current: 84,
            },
        },
        TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(6),
                experience_spent: 0,
                advancement: SkillAdvancement::Trained,
            },
            details: TraitDetails::Skill {
                initial_level: 5,
                resistance_at_last_check: 327,
                last_used_time: 1234.125,
            },
        },
    ]
}

#[test]
fn complete_projections_preserve_authoritative_metadata_through_expenditure() {
    let states = states();
    let mut character = CharacterProgression::with_state(&states, tables(), 100, 7).unwrap();
    for state in states {
        let before = character.projection(state.progress.target).unwrap();
        assert_eq!(before.details, Some(state.details));
        assert_eq!(before.advancement, state.progress.advancement);
        let change = character
            .raise(RaiseProgression {
                target: state.progress.target,
                amount: 10,
            })
            .unwrap();
        assert_eq!(change.before, before);
        assert_eq!(change.after.details, Some(state.details));
        assert_eq!(change.after.advancement, state.progress.advancement);
        assert_eq!(change.after.ranks, 2);
    }
    assert_eq!(character.available_experience(), 70);
    assert_eq!(character.revision(), 10);
    let exported: Vec<_> = character.trait_states().collect();
    assert!(exported.iter().all(|(_, details)| details.is_some()));
}

#[test]
fn missing_metadata_remains_missing_instead_of_fabricating_defaults() {
    let progress = states()[0].progress;
    let mut character = CharacterProgression::new(&[progress], tables(), 10, 0).unwrap();
    assert_eq!(character.projection(progress.target).unwrap().details, None);
    let change = character
        .raise(RaiseProgression {
            target: progress.target,
            amount: 1,
        })
        .unwrap();
    assert_eq!(change.before.details, None);
    assert_eq!(change.after.details, None);
    assert_eq!(character.trait_states().next().unwrap().1, None);
}

#[test]
fn wrong_metadata_or_nonfinite_skill_time_cannot_enter_authoritative_state() {
    for (index, replacement) in [
        (0, states()[1].details),
        (1, states()[2].details),
        (2, states()[0].details),
    ] {
        let mut states = states();
        states[index].details = replacement;
        assert_eq!(
            CharacterProgression::with_state(&states, tables(), 10, 0).unwrap_err(),
            ProgressionStateError::WrongTraitDetails
        );
    }
    for last_used_time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut states = states();
        states[2].details = TraitDetails::Skill {
            initial_level: 0,
            resistance_at_last_check: 0,
            last_used_time,
        };
        assert_eq!(
            CharacterProgression::with_state(&states, tables(), 10, 0).unwrap_err(),
            ProgressionStateError::NonfiniteSkillTime
        );
    }
}

#[test]
fn an_earlier_projection_remains_frozen_after_later_mutations() {
    let state = states()[2];
    let mut character = CharacterProgression::with_state(&[state], tables(), 100, 0).unwrap();
    let first = character
        .raise(RaiseProgression {
            target: state.progress.target,
            amount: 1,
        })
        .unwrap();
    character
        .raise(RaiseProgression {
            target: state.progress.target,
            amount: 9,
        })
        .unwrap();
    assert_eq!(first.after.experience_spent, 1);
    assert_eq!(first.after.ranks, 1);
    assert_eq!(first.after.details, Some(state.details));
    assert_eq!(
        character.projection(state.progress.target).unwrap().ranks,
        2
    );
}
