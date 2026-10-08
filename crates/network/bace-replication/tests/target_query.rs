use bace_gameplay_api::{
    ActionContext, CharacterBinding, SessionId,
    selection::{TargetQueryEvent, TargetQueryResponse},
};
use bace_replication::{BatchLimits, EventSequencer};
use bace_types::{AccountId, EntityId};
#[test]
fn query_sequence_commits_only_whole_encoded_response() {
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
    };
    let context = ActionContext {
        actor: binding.actor,
        account: binding.account,
        session: binding.session,
        sequence: 10,
    };
    let limits = BatchLimits {
        max_messages: 1,
        max_bytes: 64,
        max_message_bytes: 64,
        max_string_bytes: 64,
    };
    let mut events = EventSequencer::new(binding, 42);
    let event = TargetQueryEvent {
        context,
        response: Some(TargetQueryResponse::ItemMana {
            target: EntityId(9),
            fraction: 0.5,
            success: 1,
        }),
    };
    assert!(
        events
            .project_target_query(
                binding,
                event,
                BatchLimits {
                    max_bytes: 20,
                    ..limits
                }
            )
            .is_err()
    );
    assert_eq!(events.next_sequence(), 42);
    let result = events.project_target_query(binding, event, limits).unwrap();
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].queue, 9);
    assert_eq!(events.next_sequence(), 43);
    let empty = events
        .project_target_query(
            binding,
            TargetQueryEvent {
                response: None,
                ..event
            },
            limits,
        )
        .unwrap();
    assert!(empty.messages.is_empty());
    assert_eq!(events.next_sequence(), 43);
    let mut stale = event;
    stale.context.session = SessionId(4);
    assert!(events.project_target_query(binding, stale, limits).is_err());
    assert_eq!(events.next_sequence(), 43);
}
