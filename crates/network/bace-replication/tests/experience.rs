use bace_gameplay_api::{
    CharacterBinding, SessionId,
    experience::{ExperienceEvent, ExperienceState},
};
use bace_replication::{BatchLimits, SequenceKind, Sequences, experience::project_experience};
use bace_types::{AccountId, EntityId};
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(17),
        account: AccountId(1),
        session: SessionId(1),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 20,
        max_bytes: 4096,
        max_message_bytes: 1024,
        max_string_bytes: 1024,
    }
}
fn event(
    before_level: u32,
    before: u64,
    amount: u64,
    after_level: u32,
    after: u64,
    credits: u32,
) -> ExperienceEvent {
    ExperienceEvent {
        update_properties: true,
        actor: EntityId(17),
        before: ExperienceState {
            total: before,
            available: before,
            level: before_level,
            available_skill_credits: 0,
        },
        after: ExperienceState {
            total: after,
            available: after,
            level: after_level,
            available_skill_credits: credits,
        },
        maximum_level: 4,
        quest_amount: Some(amount),
        next_credit_level: Some(3),
        vitals: if after_level > before_level {
            vec![(2, 100), (4, 80), (6, 60)]
        } else {
            vec![]
        },
    }
}
#[test]
fn original_update_xp_and_level_methods_match_complete_ordered_bytes() {
    let cases = [
        event(1, 0, 50, 1, 50, 0),
        event(1, 0, 100, 2, 100, 0),
        event(1, 0, 350, 3, 350, 2),
        event(3, 300, 500, 4, 600, 0),
        event(4, 600, 999, 4, 600, 0),
    ];
    for e in cases {
        let key = format!(
            "{},{},{},",
            e.before.level,
            e.before.total,
            e.quest_amount.unwrap()
        );
        let expected: Vec<Vec<u8>> = include_str!("fixtures/experience.csv")
            .lines()
            .filter_map(|line| line.strip_prefix(&key))
            .map(|hex| {
                hex.as_bytes()
                    .chunks_exact(2)
                    .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                    .collect()
            })
            .collect();
        assert!(!expected.is_empty());
        let mut sequences = Sequences::new(16).unwrap();
        let batch = project_experience(binding(), &e, &mut sequences, limits()).unwrap();
        assert_eq!(
            batch
                .owner
                .messages
                .iter()
                .map(|m| m.bytes.clone())
                .collect::<Vec<_>>(),
            expected,
            "{key}"
        );
    }
}
#[test]
fn pressure_and_invalid_vitals_preserve_every_sequence() {
    let mut e = event(1, 0, 100, 2, 100, 0);
    let mut sequences = Sequences::new(16).unwrap();
    let mut small = limits();
    small.max_messages = 2;
    assert!(project_experience(binding(), &e, &mut sequences, small).is_err());
    assert_eq!(sequences.current(SequenceKind::PropertyInt64, 1), 255);
    e.vitals.push((2, 100));
    assert!(project_experience(binding(), &e, &mut sequences, limits()).is_err());
    assert_eq!(sequences.current(SequenceKind::PropertyInt, 25), 255);
    e = event(4, 600, 999, 4, 600, 0);
    e.update_properties = false;
    let batch = project_experience(binding(), &e, &mut sequences, limits()).unwrap();
    assert_eq!(batch.owner.messages.len(), 1);
    assert_eq!(sequences.current(SequenceKind::PropertyInt64, 1), 255);
}
