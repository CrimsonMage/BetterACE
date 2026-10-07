use bace_quests::{
    QuestDefinition, QuestEligibility, QuestProgress, QuestTimeError, has_bits, has_no_bits,
    has_solves, next_solve,
};

fn optional(value: &str) -> Option<i32> {
    if value == "none" {
        None
    } else {
        Some(value.parse().unwrap())
    }
}

#[test]
fn cooldown_and_predicates_match_pinned_official_methods() {
    let mut count = 0;
    for line in include_str!("fixtures/eligibility.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values: Vec<&str> = line.split(',').collect();
        match values[0] {
            "time" => {
                let definition = (values[1] == "1")
                    .then(|| QuestDefinition::new(values[3], 5, values[4].parse().unwrap()));
                let progress = (values[2] == "1").then(|| QuestProgress {
                    last_completed_seconds: 100,
                    completions: values[5].parse().unwrap(),
                });
                let outcome = next_solve(
                    definition.as_ref(),
                    progress.as_ref(),
                    values[7].parse().unwrap(),
                    values[6].parse().unwrap(),
                )
                .unwrap();
                match values[8] {
                    "ready" => assert_eq!(outcome, QuestEligibility::Ready, "{line}"),
                    "blocked" => assert!(
                        matches!(
                            outcome,
                            QuestEligibility::MissingDefinition | QuestEligibility::MaximumSolves
                        ),
                        "{line}"
                    ),
                    seconds => assert_eq!(
                        outcome,
                        QuestEligibility::Wait {
                            seconds: seconds.parse().unwrap()
                        },
                        "{line}"
                    ),
                }
            }
            "bits" => {
                let progress = (values[1] == "1").then(|| QuestProgress {
                    last_completed_seconds: 0,
                    completions: values[2].parse().unwrap(),
                });
                let bits = values[3].parse().unwrap();
                assert_eq!(
                    has_bits(progress.as_ref(), bits),
                    values[4] == "True",
                    "{line}"
                );
                assert_eq!(
                    has_no_bits(progress.as_ref(), bits),
                    values[5] == "True",
                    "{line}"
                );
            }
            "range" => {
                let progress = (values[1] == "1").then(|| QuestProgress {
                    last_completed_seconds: 0,
                    completions: values[2].parse().unwrap(),
                });
                assert_eq!(
                    has_solves(progress.as_ref(), optional(values[3]), optional(values[4])),
                    values[5] == "True",
                    "{line}"
                );
            }
            _ => panic!("unknown official fixture row"),
        }
        count += 1;
    }
    assert_eq!(count, 4610);
}

#[test]
fn invalid_rates_and_overflow_never_make_a_quest_ready() {
    let rule = QuestDefinition::new("Quest", 1, -1);
    let progress = QuestProgress {
        last_completed_seconds: 0,
        completions: 1,
    };
    for rate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001] {
        assert_eq!(
            next_solve(Some(&rule), Some(&progress), u32::MAX, rate),
            Err(QuestTimeError::InvalidRate)
        );
    }
    assert_eq!(
        next_solve(
            Some(&rule),
            Some(&progress),
            u32::MAX,
            f64::from(u32::MAX) + 1.0
        ),
        Err(QuestTimeError::DeltaOverflow)
    );
    let edge = QuestProgress {
        last_completed_seconds: u32::MAX,
        completions: 1,
    };
    assert_eq!(
        next_solve(Some(&rule), Some(&edge), u32::MAX, 1.0),
        Err(QuestTimeError::DeadlineOverflow)
    );
    assert_eq!(
        next_solve(Some(&rule), Some(&edge), u32::MAX, 0.0),
        Ok(QuestEligibility::Ready)
    );
    // Fractional conversion truncates before addition, matching C# within range.
    assert_eq!(
        next_solve(
            Some(&rule),
            Some(&progress),
            u32::MAX - 1,
            f64::from(u32::MAX) + 0.5
        ),
        Ok(QuestEligibility::Wait { seconds: 1 })
    );
}

#[test]
fn missing_definition_and_zero_solve_limit_preserve_official_ordering() {
    let rule = QuestDefinition::new("Quest", 10, 0);
    let progress = QuestProgress {
        last_completed_seconds: 100,
        completions: 0,
    };
    assert_eq!(
        next_solve(None, None, 100, 1.0),
        Ok(QuestEligibility::MissingDefinition)
    );
    assert_eq!(
        next_solve(Some(&rule), None, 100, 1.0),
        Ok(QuestEligibility::Ready)
    );
    assert_eq!(
        next_solve(Some(&rule), Some(&progress), 100, 1.0),
        Ok(QuestEligibility::MaximumSolves)
    );
}
