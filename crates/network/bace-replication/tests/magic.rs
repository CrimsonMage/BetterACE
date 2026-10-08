use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer};
use bace_types::{AccountId, EntityId};
use bace_wire::MagicEvent;
#[test]
fn magic_batch_failure_does_not_advance_shared_event_counter() {
    let binding = CharacterBinding {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(3),
    };
    let mut seq = EventSequencer::new(binding, u32::MAX);
    let events = [
        MagicEvent::Purge,
        MagicEvent::UpdateSpell { spell: 1, layer: 0 },
    ];
    let mut limits = BatchLimits {
        max_messages: 2,
        max_bytes: 16,
        max_message_bytes: 128,
        max_string_bytes: 128,
    };
    assert!(seq.project_magic(binding, &events, 10, limits).is_err());
    assert_eq!(seq.next_sequence(), u32::MAX);
    limits.max_bytes = 128;
    let batch = seq.project_magic(binding, &events, 10, limits).unwrap();
    assert_eq!(batch.messages.len(), 2);
    assert!(batch.messages.iter().all(|m| m.queue == 9));
    assert_eq!(seq.next_sequence(), 1);
}
