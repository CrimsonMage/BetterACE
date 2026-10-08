use bace_gameplay_api::{CharacterBinding, SessionId, item_experience::ItemExperienceEvent};
use bace_replication::{
    BatchLimits, SequenceKind, Sequences, item_experience::project_item_experience,
};
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
        max_messages: 3,
        max_bytes: 1024,
        max_message_bytes: 512,
        max_string_bytes: 256,
    }
}
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_ace_message_bytes_and_atomic_sequence_admission() {
    let rows: Vec<_> = include_str!("fixtures/item_experience.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split(',').collect::<Vec<_>>())
        .collect();
    for row in rows.iter().filter(|r| r[0] == "xp") {
        let sequence: u8 = row[1].parse().unwrap();
        let total = row[2].parse().unwrap();
        let mut seq = Sequences::new(1).unwrap();
        for _ in 0..sequence {
            seq.advance_batch([(SequenceKind::PropertyInt64, 4)])
                .unwrap();
        }
        let event = ItemExperienceEvent {
            actor: EntityId(17),
            item: EntityId(99),
            total,
            level_up: None,
        };
        let batch =
            project_item_experience(binding(), &event, EntityId(99), &mut seq, limits()).unwrap();
        assert_eq!(batch.owner.messages[0].bytes, bytes(row[3]));
    }
    let event = ItemExperienceEvent {
        actor: EntityId(17),
        item: EntityId(99),
        total: 10,
        level_up: Some(("Aetheria".into(), 3)),
    };
    let mut seq = Sequences::new(1).unwrap();
    let mut small = limits();
    small.max_messages = 2;
    assert!(project_item_experience(binding(), &event, EntityId(99), &mut seq, small).is_err());
    assert_eq!(seq.current(SequenceKind::PropertyInt64, 4), 255);
    let batch =
        project_item_experience(binding(), &event, EntityId(99), &mut seq, limits()).unwrap();
    assert_eq!(
        batch.owner.messages[1].bytes,
        bytes(rows.iter().find(|r| r[0] == "chat").unwrap()[3])
    );
    assert_eq!(
        batch.owner.messages[2].bytes,
        bytes(rows.iter().find(|r| r[0] == "script").unwrap()[3])
    );
    assert_eq!(batch.observers, vec![batch.owner.messages[2].clone()]);
    assert!(project_item_experience(binding(), &event, EntityId(100), &mut seq, limits()).is_err());
}
