use std::sync::Arc;

use bace_character::{
    CharacterProgression, ProgressionStateError, ProgressionTables, RankTable, RankTableError,
    TraitProgress,
};
use bace_gameplay_api::{
    AttributeId, ProgressionRejection, ProgressionTarget, RaiseProgression, SkillAdvancement,
    VitalId,
};

fn tables() -> Arc<ProgressionTables> {
    Arc::new(ProgressionTables {
        attributes: RankTable::new(&[0, 10, 30, 30, 100]).unwrap(),
        vitals: RankTable::new(&[0, 2, 8, 20, 100]).unwrap(),
        trained_skills: RankTable::new(&[0, 5, 15, 50, 100]).unwrap(),
        specialized_skills: RankTable::new(&[0, 1, 7, 40, 100]).unwrap(),
    })
}

fn trait_for(kind: u64, spent: u32) -> TraitProgress {
    let (target, advancement) = match kind {
        0 => (
            ProgressionTarget::Attribute(AttributeId::Strength),
            SkillAdvancement::Inactive,
        ),
        1 => (
            ProgressionTarget::Vital(VitalId::MaxHealth),
            SkillAdvancement::Inactive,
        ),
        2 => (ProgressionTarget::Skill(6), SkillAdvancement::Trained),
        3 => (ProgressionTarget::Skill(6), SkillAdvancement::Specialized),
        _ => panic!("invalid oracle category"),
    };
    TraitProgress {
        target,
        experience_spent: spent,
        advancement,
    }
}

#[test]
fn expenditure_matches_pinned_official_csharp_methods() {
    let fixture = include_str!("fixtures/progression.csv");
    assert!(fixture.contains("47edade3bd3f6044b676d4eb877c4965c7eda62b"));
    let tables = tables();
    let mut count = 0;
    for line in fixture.lines().filter(|line| !line.starts_with('#')) {
        let values: Vec<u64> = line.split(',').map(|part| part.parse().unwrap()).collect();
        assert_eq!(values.len(), 8);
        let entry = trait_for(values[0], values[1].try_into().unwrap());
        let mut character =
            CharacterProgression::new(&[entry], tables.clone(), values[2], 7).unwrap();
        let request = RaiseProgression {
            target: entry.target,
            amount: values[3].try_into().unwrap(),
        };
        let outcome = character.raise(request);
        assert_eq!(outcome.is_ok(), values[4] == 1, "{line}");
        let after = character.projection(entry.target).unwrap();
        assert_eq!(u64::from(after.experience_spent), values[5], "{line}");
        assert_eq!(u64::from(after.ranks), values[6], "{line}");
        assert_eq!(character.available_experience(), values[7], "{line}");
        assert_eq!(
            character.revision(),
            7 + u64::from(outcome.is_ok() && request.amount > 0),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 1408);
}

#[test]
fn validates_asset_tables_before_allocation_and_preserves_duplicate_thresholds() {
    assert_eq!(RankTable::new(&[]), Err(RankTableError::Empty));
    assert_eq!(RankTable::new(&[1]), Err(RankTableError::MissingZeroRank));
    assert_eq!(
        RankTable::new(&[0, 2, 1]),
        Err(RankTableError::DecreasingThresholds)
    );
    assert_eq!(
        RankTable::new(&vec![0; 65_537]),
        Err(RankTableError::TooManyRanks)
    );
    let largest = RankTable::new(&vec![0; 65_536]).unwrap();
    assert_eq!(largest.rank(0), u16::MAX);
    assert_eq!(tables().attributes.rank(30), 3);
    assert_eq!(tables().attributes.rank(29), 1);
    assert_eq!(tables().attributes.rank(u32::MAX), 4);
}

#[test]
fn load_validation_rejects_inconsistent_or_unbounded_state() {
    let entry = trait_for(0, 0);
    let error = |entries: &[TraitProgress], available| {
        CharacterProgression::new(entries, tables(), available, 0).unwrap_err()
    };
    assert_eq!(
        error(&[entry, entry], 0),
        ProgressionStateError::DuplicateTarget
    );
    assert_eq!(
        error(&vec![entry; 257], 0),
        ProgressionStateError::TooManyTraits
    );
    assert_eq!(
        error(&[trait_for(0, 101)], 0),
        ProgressionStateError::ExperienceBeyondMaximum
    );
    assert_eq!(
        error(&[entry], u64::MAX),
        ProgressionStateError::AvailableExperienceOutOfRange
    );
    assert_eq!(
        error(
            &[TraitProgress {
                advancement: SkillAdvancement::Trained,
                ..entry
            }],
            0
        ),
        ProgressionStateError::InvalidAdvancement
    );
    assert_eq!(
        error(
            &[TraitProgress {
                advancement: SkillAdvancement::Untrained,
                ..trait_for(2, 1)
            }],
            0
        ),
        ProgressionStateError::ExperienceOnUntrainedSkill
    );
}

#[test]
fn rejected_requests_do_not_mutate_any_progression_or_revision() {
    let entry = trait_for(0, 0);
    let skill = TraitProgress {
        advancement: SkillAdvancement::Untrained,
        ..trait_for(2, 0)
    };
    let mut character = CharacterProgression::new(&[entry, skill], tables(), 10, 0).unwrap();
    let before = character.projection(entry.target);
    for (request, expected) in [
        (
            RaiseProgression {
                target: ProgressionTarget::Skill(999),
                amount: 1,
            },
            ProgressionRejection::UnknownTarget,
        ),
        (
            RaiseProgression {
                target: skill.target,
                amount: 1,
            },
            ProgressionRejection::UntrainedSkill,
        ),
        (
            RaiseProgression {
                target: entry.target,
                amount: 11,
            },
            ProgressionRejection::InsufficientExperience,
        ),
    ] {
        assert_eq!(character.raise(request), Err(expected));
        assert_eq!(character.projection(entry.target), before);
        assert_eq!(
            character.projection(skill.target).unwrap().experience_spent,
            0
        );
        assert_eq!(character.available_experience(), 10);
        assert_eq!(character.revision(), 0);
    }
    let mut exhausted = CharacterProgression::new(&[entry], tables(), 100, u64::MAX).unwrap();
    assert_eq!(
        exhausted.raise(RaiseProgression {
            target: entry.target,
            amount: 1
        }),
        Err(ProgressionRejection::RevisionExhausted)
    );
    assert_eq!(exhausted.projection(entry.target), before);
    assert_eq!(exhausted.available_experience(), 100);
    assert_eq!(exhausted.revision(), u64::MAX);
    assert!(
        exhausted
            .raise(RaiseProgression {
                target: entry.target,
                amount: 0
            })
            .is_ok()
    );
}

#[test]
fn attribute_and_vital_ids_do_not_truncate_untrusted_words() {
    assert_eq!(AttributeId::try_from(3), Ok(AttributeId::Quickness));
    assert_eq!(AttributeId::try_from(4), Ok(AttributeId::Coordination));
    for invalid in [0, 7, 65537, u32::MAX] {
        assert!(AttributeId::try_from(invalid).is_err());
    }
    for invalid in [0, 2, 4, 6, 65537, u32::MAX] {
        assert!(VitalId::try_from(invalid).is_err());
    }
}
