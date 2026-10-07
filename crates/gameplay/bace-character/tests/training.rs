use std::sync::Arc;

use bace_character::{
    CharacterProgression, ProgressionTables, RankTable, SkillCosts, SkillRulesError,
    SkillTrainingRules, TrainingSetupError, TraitProgress, TraitState,
};
use bace_gameplay_api::{
    ProgressionTarget, SkillAdvancement, SkillTrainingChange, SkillTrainingRejection, TrainSkill,
    TraitDetails,
};

fn tables() -> Arc<ProgressionTables> {
    Arc::new(ProgressionTables {
        attributes: RankTable::new(&[0, 10, 30, 30, 100]).unwrap(),
        vitals: RankTable::new(&[0, 2, 8, 20, 100]).unwrap(),
        trained_skills: RankTable::new(&[0, 5, 15, 50, 100]).unwrap(),
        specialized_skills: RankTable::new(&[0, 1, 7, 40, 100]).unwrap(),
    })
}
fn state(advancement: SkillAdvancement, experience_spent: u32, initial_level: u32) -> TraitState {
    TraitState {
        progress: TraitProgress {
            target: ProgressionTarget::Skill(0),
            experience_spent,
            advancement,
        },
        details: TraitDetails::Skill {
            initial_level,
            resistance_at_last_check: 23,
            last_used_time: 456.5,
        },
    }
}
fn character(
    advancement: SkillAdvancement,
    experience_spent: u32,
    initial_level: u32,
) -> CharacterProgression {
    CharacterProgression::with_state(
        &[state(advancement, experience_spent, initial_level)],
        tables(),
        100,
        5,
    )
    .unwrap()
}
fn rules(trained_cost: i32, specialized_cost: i32) -> Arc<SkillTrainingRules> {
    Arc::new(
        SkillTrainingRules::new(&[SkillCosts {
            skill: 0,
            trained_cost,
            specialized_cost,
        }])
        .unwrap(),
    )
}

#[test]
fn training_and_preserving_specialization_match_official_csharp() {
    let mut count = 0;
    for line in include_str!("fixtures/training.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let (kind, values) = line.split_once(',').unwrap();
        let values: Vec<i64> = values
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let advancement = SkillAdvancement::try_from(values[0] as u32).unwrap();
        let mut character = character(advancement, values[4] as u32, values[5] as u32)
            .with_training(
                rules(values[2] as i32, values[2] as i32),
                values[1] as u32,
                &[],
            )
            .unwrap();
        let result = match kind {
            "train" => character.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: values[3] as i32,
            }),
            "specialize" => character.specialize_skill(0),
            _ => panic!("unknown official fixture action"),
        };
        assert_eq!(result.is_ok(), values[6] == 1, "{line}");
        let actual = character.projection(ProgressionTarget::Skill(0)).unwrap();
        assert_eq!(actual.advancement as i64, values[7], "{line}");
        assert_eq!(i64::from(actual.experience_spent), values[8], "{line}");
        assert_eq!(i64::from(actual.ranks), values[9], "{line}");
        assert_eq!(
            actual.details,
            Some(TraitDetails::Skill {
                initial_level: values[10] as u32,
                resistance_at_last_check: 23,
                last_used_time: 456.5
            }),
            "{line}"
        );
        assert_eq!(
            i64::from(character.available_skill_credits().unwrap()),
            values[11],
            "{line}"
        );
        assert_eq!(character.available_experience(), 100, "{line}");
        assert_eq!(
            character.revision(),
            5 + u64::from(result.is_ok()),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 270);
}

fn unchanged(
    mut character: CharacterProgression,
    action: impl FnOnce(
        &mut CharacterProgression,
    ) -> Result<SkillTrainingChange, SkillTrainingRejection>,
    expected: SkillTrainingRejection,
) {
    let before: Vec<_> = character.trait_states().collect();
    let credits = character.available_skill_credits();
    let xp = character.available_experience();
    let revision = character.revision();
    assert_eq!(action(&mut character), Err(expected));
    assert_eq!(character.trait_states().collect::<Vec<_>>(), before);
    assert_eq!(character.available_skill_credits(), credits);
    assert_eq!(character.available_experience(), xp);
    assert_eq!(character.revision(), revision);
}

#[test]
fn every_failed_training_gate_preserves_owned_state() {
    let untrained = || character(SkillAdvancement::Untrained, 0, 123);
    let prepared = || untrained().with_training(rules(4, 6), 20, &[]).unwrap();
    unchanged(
        untrained(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::Unavailable,
    );
    unchanged(
        prepared(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 99,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::UnknownSkill,
    );
    unchanged(
        prepared(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: -4,
            })
        },
        SkillTrainingRejection::NegativeQuotedCost,
    );
    unchanged(
        prepared(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 3,
            })
        },
        SkillTrainingRejection::PriceMismatch,
    );
    unchanged(
        untrained().with_training(rules(4, 6), 3, &[]).unwrap(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::InsufficientCredits,
    );
    unchanged(
        character(SkillAdvancement::Trained, 9, 123)
            .with_training(rules(4, 6), 20, &[])
            .unwrap(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::AlreadyTrained,
    );
    unchanged(
        untrained().with_training(rules(4, 6), 20, &[0]).unwrap(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::UnsupportedAugmentation,
    );
    let incomplete = CharacterProgression::new(
        &[state(SkillAdvancement::Untrained, 0, 123).progress],
        tables(),
        100,
        5,
    )
    .unwrap();
    unchanged(
        incomplete.with_training(rules(4, 6), 20, &[]).unwrap(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::MissingTraitDetails,
    );
    let exhausted = CharacterProgression::with_state(
        &[state(SkillAdvancement::Untrained, 0, 123)],
        tables(),
        100,
        u64::MAX,
    )
    .unwrap();
    unchanged(
        exhausted.with_training(rules(4, 6), 20, &[]).unwrap(),
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::RevisionExhausted,
    );
}

#[test]
fn newly_instantiated_known_skill_uses_official_defaults_but_no_creation_bonus() {
    let mut character = CharacterProgression::new(&[], tables(), 100, 5)
        .unwrap()
        .with_training(rules(4, 6), 20, &[])
        .unwrap();
    let change = character
        .train_skill(TrainSkill {
            skill: 0,
            quoted_credits: 4,
        })
        .unwrap();
    assert_eq!(change.before.advancement, SkillAdvancement::Untrained);
    assert_eq!(change.after.advancement, SkillAdvancement::Trained);
    assert_eq!(change.after.ranks, 0);
    assert_eq!(change.after.experience_spent, 0);
    assert_eq!(
        change.after.details,
        Some(TraitDetails::Skill {
            initial_level: 0,
            resistance_at_last_check: 0,
            last_used_time: 0.0
        })
    );
    assert_eq!(change.available_skill_credits, 16);
    assert_eq!(change.revision, 6);
}

#[test]
fn specialization_preserves_xp_uses_specialized_table_and_retains_usage_metadata() {
    let mut character = character(SkillAdvancement::Trained, 9, 123)
        .with_training(rules(4, 6), 20, &[])
        .unwrap();
    let change = character.specialize_skill(0).unwrap();
    assert_eq!(change.before.ranks, 1);
    assert_eq!(change.after.ranks, 2);
    assert_eq!(change.after.experience_spent, 9);
    assert_eq!(
        change.after.details,
        Some(TraitDetails::Skill {
            initial_level: 10,
            resistance_at_last_check: 23,
            last_used_time: 456.5
        })
    );
    assert_eq!(change.available_skill_credits, 14);
    unchanged(
        character,
        |c| c.specialize_skill(0),
        SkillTrainingRejection::NotTrained,
    );
}

#[test]
fn invalid_training_setup_returns_aggregate_and_never_installs_partial_config() {
    for (credits, augmentations, expected) in [
        (u32::MAX, vec![], TrainingSetupError::InvalidCredits),
        (20, vec![99], TrainingSetupError::UnknownAugmentationSkill),
        (
            20,
            vec![0, 0],
            TrainingSetupError::DuplicateAugmentationSkill,
        ),
        (20, vec![0; 257], TrainingSetupError::TooManyAugmentations),
    ] {
        let (error, returned) = character(SkillAdvancement::Untrained, 0, 123)
            .with_training(rules(4, 6), credits, &augmentations)
            .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(returned.available_skill_credits(), None);
        assert_eq!(returned.revision(), 5);
        assert_eq!(returned.available_experience(), 100);
    }
    let prepared = character(SkillAdvancement::Untrained, 0, 123)
        .with_training(rules(4, 6), 20, &[])
        .unwrap();
    let (error, retained) = prepared.with_training(rules(0, 0), 999, &[]).unwrap_err();
    assert_eq!(error, TrainingSetupError::AlreadyConfigured);
    assert_eq!(retained.available_skill_credits(), Some(20));
}

#[test]
fn invalid_signed_prices_and_unbounded_tables_are_rejected_before_use() {
    for (trained_cost, specialized_cost) in [(-1, 0), (0, -1), (i32::MIN, i32::MAX)] {
        assert!(matches!(
            SkillTrainingRules::new(&[SkillCosts {
                skill: 0,
                trained_cost,
                specialized_cost
            }]),
            Err(SkillRulesError::NegativeCost)
        ));
    }
    let cost = SkillCosts {
        skill: 0,
        trained_cost: 0,
        specialized_cost: 0,
    };
    assert!(matches!(
        SkillTrainingRules::new(&[cost, cost]),
        Err(SkillRulesError::DuplicateSkill)
    ));
    assert!(matches!(
        SkillTrainingRules::new(&[cost; 257]),
        Err(SkillRulesError::TooManySkills)
    ));
}

#[test]
fn new_skill_cannot_exceed_trait_capacity_or_mutate_on_specialization_failures() {
    let states: Vec<_> = (1..=256)
        .map(|id| {
            let mut state = state(SkillAdvancement::Untrained, 0, 0);
            state.progress.target = ProgressionTarget::Skill(id);
            state
        })
        .collect();
    let full = CharacterProgression::with_state(&states, tables(), 100, 5)
        .unwrap()
        .with_training(rules(4, 6), 20, &[])
        .unwrap();
    unchanged(
        full,
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::TraitCapacity,
    );
    unchanged(
        character(SkillAdvancement::Untrained, 0, 0)
            .with_training(rules(4, 6), 20, &[])
            .unwrap(),
        |c| c.specialize_skill(0),
        SkillTrainingRejection::NotTrained,
    );
    unchanged(
        character(SkillAdvancement::Trained, 9, 0)
            .with_training(rules(4, 6), 5, &[])
            .unwrap(),
        |c| c.specialize_skill(0),
        SkillTrainingRejection::InsufficientCredits,
    );
    unchanged(
        character(SkillAdvancement::Trained, 9, 0)
            .with_training(rules(4, 6), 20, &[0])
            .unwrap(),
        |c| c.specialize_skill(0),
        SkillTrainingRejection::UnsupportedAugmentation,
    );
}

#[test]
fn malformed_zero_rank_tables_and_specialization_caps_fail_without_spending() {
    let mut malformed = (*tables()).clone();
    malformed.trained_skills = RankTable::new(&[0, 0, 100]).unwrap();
    let character = CharacterProgression::new(&[], Arc::new(malformed), 100, 5).unwrap();
    let (error, returned) = character.with_training(rules(4, 6), 20, &[]).unwrap_err();
    assert_eq!(error, TrainingSetupError::UnsupportedZeroRankTable);
    assert_eq!(returned.available_skill_credits(), None);
    let mut narrow = (*tables()).clone();
    narrow.specialized_skills = RankTable::new(&[0, 5]).unwrap();
    let capped = CharacterProgression::with_state(
        &[state(SkillAdvancement::Trained, 9, 0)],
        Arc::new(narrow),
        100,
        5,
    )
    .unwrap()
    .with_training(rules(4, 6), 20, &[])
    .unwrap();
    unchanged(
        capped,
        |c| c.specialize_skill(0),
        SkillTrainingRejection::ExperienceBeyondMaximum,
    );
    let exhausted = CharacterProgression::with_state(
        &[state(SkillAdvancement::Trained, 9, 0)],
        tables(),
        100,
        u64::MAX,
    )
    .unwrap()
    .with_training(rules(4, 6), 20, &[])
    .unwrap();
    unchanged(
        exhausted,
        |c| c.specialize_skill(0),
        SkillTrainingRejection::RevisionExhausted,
    );
    let empty = CharacterProgression::new(&[], tables(), 100, u64::MAX)
        .unwrap()
        .with_training(rules(4, 6), 20, &[])
        .unwrap();
    unchanged(
        empty,
        |c| {
            c.train_skill(TrainSkill {
                skill: 0,
                quoted_credits: 4,
            })
        },
        SkillTrainingRejection::RevisionExhausted,
    );
}
