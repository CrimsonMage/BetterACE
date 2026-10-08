use bace_gameplay_api::{ActionContext, CharacterBinding, SessionId, staff::StaffEvent};
use bace_replication::{BatchLimits, EventSequencer};
use bace_types::{AccountId, EntityId};
fn event(case: u32) -> StaffEvent {
    StaffEvent::Spellbook {
        context: ActionContext {
            actor: EntityId(17),
            account: AccountId(1),
            session: SessionId(1),
            sequence: 1,
        },
        spell: 20,
        name: "Test Spell".into(),
        learn: case < 2,
        changed: case == 0 || case == 2,
        revision: 2,
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 16,
        max_bytes: 4096,
        max_message_bytes: 2048,
        max_string_bytes: 1024,
    }
}
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(17),
        account: AccountId(1),
        session: SessionId(1),
    }
}
#[test]
fn original_staff_learn_and_remove_methods_match_all_ordered_bytes() {
    for case in 0..4 {
        let mut events = EventSequencer::new(binding(), 42);
        let batch = events
            .project_staff_spellbook(binding(), &event(case), limits())
            .unwrap();
        let expected = include_str!("fixtures/staff_spellbook.csv")
            .lines()
            .filter(|v| !v.starts_with('#'))
            .filter_map(|line| {
                let fields = line.split(',').collect::<Vec<_>>();
                (fields[0].parse::<u32>().unwrap() == case).then(|| {
                    (
                        fields[1].parse::<u16>().unwrap(),
                        (0..fields[2].len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&fields[2][i..i + 2], 16).unwrap())
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            batch
                .messages
                .iter()
                .map(|m| (m.queue, m.bytes.clone()))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(events.next_sequence(), if case == 0 { 43 } else { 42 });
    }
}
#[test]
fn rejected_spellbook_batch_does_not_consume_event_sequence() {
    let mut events = EventSequencer::new(binding(), 42);
    let mut small = limits();
    small.max_messages = 1;
    assert!(
        events
            .project_staff_spellbook(binding(), &event(0), small)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 42);
    assert_eq!(
        events
            .project_staff_spellbook(binding(), &event(0), limits())
            .unwrap()
            .messages
            .len(),
        3
    );
}
