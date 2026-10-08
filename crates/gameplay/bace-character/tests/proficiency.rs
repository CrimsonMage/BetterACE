use bace_character::{
    CharacterLevelTable, CharacterProgression, CharacterServiceState, ProficiencyUse,
    ProgressionTables, RankTable, TraitProgress, TraitState,
};
use bace_gameplay_api::{ProgressionTarget, SkillAdvancement, TraitDetails};
use std::sync::Arc;
fn actor(ac: u32, spent: u32, available: u64, revision: u64) -> CharacterProgression {
    let table = RankTable::new(&[0, 100, 1000, 10000]).unwrap();
    CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(18),
                experience_spent: spent,
                advancement: match ac {
                    1 => SkillAdvancement::Untrained,
                    2 => SkillAdvancement::Trained,
                    _ => SkillAdvancement::Specialized,
                },
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 100,
                last_used_time: 10000.,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        available,
        revision,
    )
    .unwrap()
}
fn services(total: u64) -> CharacterServiceState {
    CharacterServiceState {
        level: if total == 100000 {
            3
        } else if total >= 50000 {
            2
        } else {
            1
        },
        total_experience: total,
        titles: vec![],
        enlightenment: 0,
        sanctuary: None,
        total_skill_credits: Some(0),
    }
}
fn levels() -> CharacterLevelTable {
    CharacterLevelTable::prepare(vec![0, 0, 50000, 100000], vec![0; 4]).unwrap()
}
#[test]
fn original_proficiency_matches_queued_grant_order_and_all_gates() {
    let fixture = include_str!("fixtures/proficiency.csv");
    assert!(fixture.contains("47edade3bd3f6044b676d4eb877c4965c7eda62b"));
    let mut count = 0;
    for line in fixture.lines().filter(|s| s.starts_with("CASE,")) {
        let c: Vec<_> = line.split(',').collect();
        let n = |i: usize| c[i].parse::<u64>().unwrap();
        let ac = n(2) as u32;
        let difficulty = n(4) as u32;
        let mut state = actor(
            ac,
            if ac >= 2 && difficulty == 9999 {
                9999
            } else {
                0
            },
            n(5),
            7,
        );
        let mut service = services(n(6));
        let usage = ProficiencyUse {
            skill: 18,
            difficulty,
            unix_time: 10000. + c[3].parse::<f64>().unwrap(),
            olthoi: c[1] == "True",
        };
        let change = state
            .propose_proficiency(&service, &levels(), usage, false)
            .unwrap();
        if let Some(change) = change {
            assert_eq!(change.grants_experience, c[13] == "True", "{line}");
            assert_eq!(change.earned.credited, n(14), "{line}");
            state
                .adopt_proficiency(&mut service, &levels(), &change)
                .unwrap();
        } else {
            assert_eq!(c[13], "False", "{line}");
        }
        let after = state.projection(ProgressionTarget::Skill(18)).unwrap();
        let Some(TraitDetails::Skill {
            resistance_at_last_check,
            last_used_time,
            ..
        }) = after.details
        else {
            panic!()
        };
        assert_eq!(
            resistance_at_last_check,
            u32::try_from(n(7)).unwrap(),
            "{line}"
        );
        assert_eq!(last_used_time, c[8].parse::<f64>().unwrap(), "{line}");
        assert_eq!(after.experience_spent, n(9) as u32, "{line}");
        assert_eq!(
            (
                service.total_experience,
                state.available_experience(),
                u64::from(service.level)
            ),
            (n(10), n(11), n(12)),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 1512);
}
#[test]
fn proposal_is_atomic_and_stale_or_clock_overflow_rejects() {
    let mut state = actor(2, 0, 1000, 7);
    let mut service = services(0);
    let usage = ProficiencyUse {
        skill: 18,
        difficulty: 500,
        unix_time: 11000.,
        olthoi: false,
    };
    let change = state
        .propose_proficiency(&service, &levels(), usage, true)
        .unwrap()
        .unwrap();
    assert_eq!(state.available_experience(), 1000);
    assert_eq!(
        (change.spent, change.earned.experience.after_available),
        (500, 1050)
    );
    assert_eq!(change.earned.experience.after_revision, 8);
    state.touch_revision().unwrap();
    assert!(
        state
            .adopt_proficiency(&mut service, &levels(), &change)
            .is_err()
    );
    assert_eq!(state.available_experience(), 1000);
    assert_eq!(service.total_experience, 0);
    assert!(
        actor(2, 0, 1000, u64::MAX)
            .propose_proficiency(&service, &levels(), usage, false)
            .is_err()
    );
    assert!(
        state
            .propose_proficiency(
                &service,
                &levels(),
                ProficiencyUse {
                    unix_time: f64::NAN,
                    ..usage
                },
                false
            )
            .is_err()
    );
}
