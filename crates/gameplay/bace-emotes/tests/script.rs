use bace_emotes::{EmoteAction, EmoteError, EmoteScript, EmoteStep};
#[test]
fn rewards_block_later_quest_mutations_until_acknowledged() {
    let mut script = EmoteScript::new(
        vec![
            EmoteAction::AwardXp(100),
            EmoteAction::StampQuest("hunt".into()),
            EmoteAction::End,
        ],
        0,
        20,
    )
    .unwrap();
    assert!(matches!(
        script.step(0).unwrap(),
        EmoteStep::Effect { sequence: 1, .. }
    ));
    assert_eq!(script.step(0), Err(EmoteError::AwaitingAcknowledgment));
    assert_eq!(
        script.acknowledge(2, true),
        Err(EmoteError::WrongAcknowledgment)
    );
    script.acknowledge(1, false).unwrap();
    assert_eq!(script.step(1), Err(EmoteError::Cancelled));
}
#[test]
fn delays_and_recursive_predicates_are_bounded() {
    let mut script = EmoteScript::new(
        vec![
            EmoteAction::DelayTicks(3),
            EmoteAction::Branch {
                predicate: 7,
                when_true: 1,
                when_false: 2,
            },
            EmoteAction::End,
        ],
        4,
        3,
    )
    .unwrap();
    assert_eq!(script.step(4).unwrap(), EmoteStep::Waiting);
    assert_eq!(script.step(6).unwrap(), EmoteStep::Waiting);
    assert_eq!(
        script.step(7).unwrap(),
        EmoteStep::Predicate {
            sequence: 1,
            predicate: 7
        }
    );
    script.resolve_predicate(1, 7, true).unwrap();
    assert_eq!(
        script.step(7).unwrap(),
        EmoteStep::Predicate {
            sequence: 2,
            predicate: 7
        }
    );
    script.resolve_predicate(2, 7, true).unwrap();
    assert_eq!(script.step(7), Err(EmoteError::Budget));
}
#[test]
fn predicates_cannot_skip_delay_or_replay_an_old_answer() {
    let mut script = EmoteScript::new(
        vec![
            EmoteAction::DelayTicks(10),
            EmoteAction::Branch {
                predicate: 1,
                when_true: 1,
                when_false: 2,
            },
            EmoteAction::End,
        ],
        0,
        10,
    )
    .unwrap();
    script.step(0).unwrap();
    assert_eq!(
        script.resolve_predicate(0, 1, true),
        Err(EmoteError::InvalidBranch)
    );
    assert_eq!(script.step(9).unwrap(), EmoteStep::Waiting);
    assert_eq!(
        script.step(10).unwrap(),
        EmoteStep::Predicate {
            sequence: 1,
            predicate: 1
        }
    );
    script.resolve_predicate(1, 1, true).unwrap();
    assert_eq!(
        script.step(10).unwrap(),
        EmoteStep::Predicate {
            sequence: 2,
            predicate: 1
        }
    );
    assert_eq!(
        script.resolve_predicate(1, 1, false),
        Err(EmoteError::InvalidBranch)
    );
    script.resolve_predicate(2, 1, false).unwrap();
    assert_eq!(script.step(10).unwrap(), EmoteStep::Complete);
}
