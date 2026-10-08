use bace_character::*;
use std::sync::Arc;
#[test]
fn original_recipient_xp_level_and_credit_updates_adopt_as_one_revision() {
    let level_table = CharacterLevelTable::prepare(vec![0, 0, 100, 300], vec![0, 0, 1, 2]).unwrap();
    for row in include_str!("fixtures/earned.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = row.split(',').collect();
        let level: u32 = p[0].parse().unwrap();
        let amount = p[1].parse().unwrap();
        let rank = RankTable::new(&[0, 10]).unwrap();
        let mut progression = CharacterProgression::new(
            &[],
            Arc::new(ProgressionTables {
                attributes: rank.clone(),
                vitals: rank.clone(),
                trained_skills: rank.clone(),
                specialized_skills: rank,
            }),
            50,
            0,
        )
        .unwrap()
        .with_training(Arc::new(SkillTrainingRules::new(&[]).unwrap()), 5, &[])
        .unwrap();
        let mut state = CharacterServiceState {
            level,
            total_skill_credits: p[8].parse().ok(),
            total_experience: match level {
                1 => 0,
                2 => 100,
                _ => 300,
            },
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
        };
        let before = state.clone();
        let change = progression
            .propose_earned_experience(&state, &level_table, amount)
            .unwrap();
        assert_eq!(state, before);
        assert_eq!(progression.available_experience(), 50);
        progression
            .adopt_earned_experience(&mut state, change)
            .unwrap();
        assert_eq!(state.level, p[2].parse::<u32>().unwrap(), "{row}");
        assert_eq!(
            state.total_experience,
            p[3].parse::<u64>().unwrap(),
            "{row}"
        );
        assert_eq!(
            progression.available_experience(),
            p[4].parse::<u64>().unwrap(),
            "{row}"
        );
        assert_eq!(
            progression.available_skill_credits(),
            Some(p[5].parse().unwrap()),
            "{row}"
        );
        assert_eq!(state.total_skill_credits, p[6].parse::<i32>().ok(), "{row}");
    }
}

#[test]
fn total_credit_overflow_and_wrong_receipt_preserve_all_owner_state() {
    let rank = RankTable::new(&[0, 10]).unwrap();
    let mut progression = CharacterProgression::new(
        &[],
        Arc::new(ProgressionTables {
            attributes: rank.clone(),
            vitals: rank.clone(),
            trained_skills: rank.clone(),
            specialized_skills: rank,
        }),
        50,
        0,
    )
    .unwrap()
    .with_training(Arc::new(SkillTrainingRules::new(&[]).unwrap()), 5, &[])
    .unwrap();
    let table = CharacterLevelTable::prepare(vec![0, 0, 100], vec![0, 0, 1]).unwrap();
    let mut state = CharacterServiceState {
        level: 1,
        total_experience: 0,
        titles: vec![],
        enlightenment: 0,
        sanctuary: None,
        total_skill_credits: Some(i32::MAX),
    };
    let before = state.clone();
    assert_eq!(
        progression.propose_earned_experience(&state, &table, 100),
        Err(CharacterServiceError::Overflow)
    );
    assert_eq!(state, before);
    assert_eq!(progression.revision(), 0);
    state.total_skill_credits = Some(10);
    let mut change = progression
        .propose_earned_experience(&state, &table, 100)
        .unwrap();
    change.services.after.total_skill_credits = Some(12);
    let before = state.clone();
    assert_eq!(
        progression.adopt_earned_experience(&mut state, change),
        Err(CharacterServiceError::Conflict)
    );
    assert_eq!(state, before);
    assert_eq!(progression.available_experience(), 50);
    assert_eq!(progression.available_skill_credits(), Some(5));
    assert_eq!(progression.revision(), 0);
}
