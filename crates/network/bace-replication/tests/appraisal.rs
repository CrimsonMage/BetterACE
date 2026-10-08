use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer};
use bace_types::{AccountId, EntityId};
use bace_wire::{AppraisalLimits, AppraisalProfile};

#[test]
fn identify_response_uses_one_private_reliable_event_sequence() {
    let binding = CharacterBinding {
        actor: EntityId(0x5000_0001),
        account: AccountId(1),
        session: SessionId(7),
    };
    let codec = AppraisalLimits {
        table_entries: 128,
        string_bytes: 4096,
        message_bytes: 4096,
    };
    let profile = AppraisalProfile::default();
    let mut events = EventSequencer::new(binding, 41);
    let blocked = BatchLimits {
        max_messages: 0,
        max_bytes: 4096,
        max_message_bytes: 4096,
        max_string_bytes: 4096,
    };
    assert!(
        events
            .project_appraisal(binding, 0x8000_0001, &profile, codec, blocked)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 41);
    let limits = BatchLimits {
        max_messages: 1,
        ..blocked
    };
    let batch = events
        .project_appraisal(binding, 0x8000_0001, &profile, codec, limits)
        .unwrap();
    assert_eq!(batch.messages.len(), 1);
    assert_eq!(batch.messages[0].queue, 9);
    assert_eq!(
        batch.messages[0].bytes,
        profile
            .encode(binding.actor.0, 41, 0x8000_0001, codec)
            .unwrap()
    );
    assert_eq!(events.next_sequence(), 42);
    assert!(
        events
            .project_appraisal(binding, 0, &profile, codec, limits)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 42);
}
