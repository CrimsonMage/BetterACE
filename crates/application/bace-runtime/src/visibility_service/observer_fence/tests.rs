use super::*;
use bace_gameplay_api::SessionId;
use bace_types::AccountId;
fn fixture() -> (VisibilityService, SessionKey, CharacterBinding) {
    let key = SessionKey {
        id: 1,
        generation: 4,
    };
    let binding = CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
    };
    let mut service = VisibilityService::new(VisibilityLimits {
        observers: 2,
        objects: 8,
        known_per_observer: 8,
        batch_bytes: 512,
        retained_bytes: 8192,
        codec: ObjectCodecLimits {
            max_message_bytes: 512,
            max_model_entries: 32,
            max_children: 8,
            max_restrictions: 8,
            max_motion_commands: 8,
            max_string_bytes: 64,
        },
    })
    .unwrap();
    service.bind(key, binding).unwrap();
    (service, key, binding)
}
#[test]
fn fenced_read_cancellation_preserves_known_audience_and_does_not_project_stale_pose() {
    let (mut service, key, binding) = fixture();
    let snapshot = VisibilitySnapshot {
        binding,
        observer_epoch: 0,
        tick: 3,
        candidates: vec![VisibilityCandidate {
            entity: EntityId(9),
            distance_squared: 1.,
        }],
    };
    let observer = service.observers.get_mut(&key).unwrap();
    let ticket = observer.knowledge.stage(snapshot.clone()).unwrap().ticket;
    observer.knowledge.commit(ticket).unwrap();
    observer
        .knowledge
        .stage(VisibilitySnapshot {
            tick: 4,
            ..snapshot.clone()
        })
        .unwrap();
    observer.view_query = Some(27);
    service.cancel_view_read(28).unwrap();
    assert_eq!(service.observers[&key].view_query, Some(27));
    service.cancel_view_read(27).unwrap();
    assert!(service.knows(key, EntityId(9)));
    assert!(service.observers[&key].knowledge.pending().is_none());
    assert!(service.observers[&key].publication.is_none());
    assert_eq!(service.observers[&key].buffer.len(), 1);
    // The exact read is retired before any ObjectProjection/sequence owner call.
    assert!(service.objects.is_empty());
    service.observers.get_mut(&key).unwrap().query = Some(29);
    service
        .cancel_visibility_read(VisibilityOutcome {
            correlation: 29,
            result: Ok(snapshot),
        })
        .unwrap();
    assert!(service.observers[&key].query.is_none());
    assert!(service.knows(key, EntityId(9)));
}
#[test]
fn distinct_teleports_same_tick_reset_independently_and_retry_is_idempotent() {
    let (mut service, key, binding) = fixture();
    service.reset_observer_event(key, 3, 10).unwrap();
    let first = service.observers[&key]
        .knowledge
        .pending()
        .map(|d| d.ticket);
    // Empty reset publications commit synchronously; same event never starts again.
    service.reset_observer_event(key, 3, 10).unwrap();
    assert_eq!(
        service.observers[&key]
            .knowledge
            .pending()
            .map(|d| d.ticket),
        first
    );
    let observer = service.observers.get_mut(&key).unwrap();
    let snapshot = VisibilitySnapshot {
        binding,
        observer_epoch: 0,
        tick: 3,
        candidates: vec![VisibilityCandidate {
            entity: EntityId(9),
            distance_squared: 1.,
        }],
    };
    let ticket = observer.knowledge.stage(snapshot).unwrap().ticket;
    observer.knowledge.commit(ticket).unwrap();
    assert!(service.knows(key, EntityId(9)));
    service.reset_observer_event(key, 3, 11).unwrap();
    assert!(!service.knows(key, EntityId(9))); // no sent blueprint means no delete bytes required
    assert_eq!(
        service.reset_observer_event(key, 3, 10),
        Err(VisibilityServiceError::Stale)
    );
    assert_eq!(
        service.reset_observer_event(key, 4, 11),
        Err(VisibilityServiceError::Stale)
    );
}
