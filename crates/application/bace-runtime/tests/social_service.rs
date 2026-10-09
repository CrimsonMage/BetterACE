use bace_admin::{ChatFeed, FeedChannel};
use bace_config::ChatApiConfig;
use bace_gameplay_api::{
    ActionContext, CharacterBinding, SessionId,
    social::{AcceptedChat, ChatChannel, ChatDelivery, SocialEvent},
};
use bace_replication::{BatchLimits, EventSequencer, Sequences};
use bace_runtime::{
    gameplay_dispatch::{GameplayDispatch, decode_social_gameplay},
    network::NetworkCommand,
    social_service::{SocialService, project_social_for_session},
};
use bace_session::{SessionKey, SessionState};
use bace_types::{AccountId, EntityId};
#[test]
fn accepted_admission_friend_event_waits_for_exact_entered_receipt() {
    let mut service = SocialService::new(None);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender
        .send(SocialEvent::Friends {
            recipient: EntityId(1),
            kind: 0,
            entries: vec![],
        })
        .unwrap();
    let mut projected = 0;
    let waiting = service
        .pump_events_ready(
            &receiver,
            4,
            |recipient| {
                assert_eq!(recipient, EntityId(1));
                false
            },
            |_, _| {
                projected += 1;
                Ok(None)
            },
            |_| Ok(()),
        )
        .unwrap();
    assert!(waiting.blocked);
    assert!(service.has_pending());
    assert_eq!(projected, 0);
    let entered = service
        .pump_events_ready(
            &receiver,
            4,
            |_| true,
            |_, event| {
                assert!(matches!(event, SocialEvent::Friends { kind: 0, .. }));
                projected += 1;
                Ok(None)
            },
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(entered.events, 1);
    assert_eq!(projected, 1);
    assert!(!service.has_pending());
}
#[tokio::test]
async fn accepted_public_feed_and_exact_network_batch_survive_pressure_without_duplicate_projection()
 {
    let (feed, publisher, mut consumer) = ChatFeed::new(ChatApiConfig::default()).unwrap();
    let mut service = SocialService::new(Some(publisher));
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    let event = SocialEvent::Chat {
        accepted: AcceptedChat {
            sequence: 1,
            unix_seconds: 123,
            sender: EntityId(1),
            sender_name: "Rune".into(),
            channel: ChatChannel::General,
            text: "live general".into(),
        },
        wire: ChatDelivery::LegacyChannel(1),
        recipients: vec![EntityId(1)],
    };
    sender.send(event).unwrap();
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(7),
    };
    let key = SessionKey {
        id: 1,
        generation: 7,
    };
    let mut events = EventSequencer::new(binding, 42);
    let mut properties = Sequences::new(16).unwrap();
    let mut projections = 0;
    let limits = BatchLimits {
        max_messages: 16,
        max_bytes: 4096,
        max_message_bytes: 2048,
        max_string_bytes: 1024,
    };
    let first = service
        .pump_events(
            &receiver,
            4,
            |_, event| {
                projections += 1;
                project_social_for_session(
                    key,
                    binding,
                    event,
                    &mut events,
                    &mut properties,
                    limits,
                )
                .map(Some)
            },
            Err,
        )
        .unwrap();
    assert!(first.blocked);
    assert_eq!(projections, 1);
    consumer.drain(16).unwrap();
    assert_eq!(
        feed.read(FeedChannel::General, None, 16)
            .unwrap()
            .events
            .len(),
        1
    );
    let mut sent = 0;
    service
        .pump_events(
            &receiver,
            4,
            |_, _| panic!("must retain already projected exact batch"),
            |command| {
                assert!(matches!(command, NetworkCommand::SendOrderedBatch { .. }));
                sent += 1;
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(sent, 1);
    assert!(!service.has_pending());
    consumer.drain(16).unwrap();
    assert_eq!(
        feed.read(FeedChannel::General, None, 16)
            .unwrap()
            .events
            .len(),
        1
    );
    sender
        .send(SocialEvent::Chat {
            accepted: AcceptedChat {
                sequence: 2,
                unix_seconds: 124,
                sender: EntityId(1),
                sender_name: "Rune".into(),
                channel: ChatChannel::Tell,
                text: "private".into(),
            },
            wire: ChatDelivery::Tell,
            recipients: vec![],
        })
        .unwrap();
    service
        .pump_events(&receiver, 4, |_, _| panic!("empty recipients"), |_| Ok(()))
        .unwrap();
    consumer.drain(16).unwrap();
    assert_eq!(
        feed.read(FeedChannel::General, None, 16)
            .unwrap()
            .events
            .len(),
        1
    );
}
#[test]
fn talk_at_command_is_intercepted_before_social_owner() {
    let mut bytes = 0xf7b1u32.to_le_bytes().to_vec();
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(0x15u32.to_le_bytes());
    let text = b"@run check";
    bytes.extend((text.len() as u16).to_le_bytes());
    bytes.extend(text);
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let context = ActionContext {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(7),
        sequence: 0,
    };
    assert!(matches!(
        decode_social_gameplay(SessionState::WorldConnected, context, &bytes, 8192, 4096).unwrap(),
        GameplayDispatch::StaffLine {
            context: ActionContext { sequence: 1, .. },
            ..
        }
    ));
    assert!(
        decode_social_gameplay(SessionState::AuthConnected, context, &bytes, 8192, 4096).is_err()
    );
}

#[test]
fn audit_feed_backpressure_retains_event_and_projects_in_game_once() {
    let (feed, publisher, mut consumer) = ChatFeed::new(ChatApiConfig {
        publication_capacity: 1,
        ..Default::default()
    })
    .unwrap();
    let accepted = |sequence, text: &str| AcceptedChat {
        sequence,
        unix_seconds: 123,
        sender: EntityId(1),
        sender_name: "Sentinel".into(),
        channel: ChatChannel::Audit,
        text: text.into(),
    };
    publisher.try_publish_audit(&accepted(1, "prior")).unwrap();
    let mut service = SocialService::new(Some(publisher));
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender
        .send(SocialEvent::Chat {
            accepted: accepted(2, "Banned account Target."),
            wire: ChatDelivery::LegacyChannel(4),
            recipients: vec![EntityId(1)],
        })
        .unwrap();
    let mut projections = 0;
    let blocked = service
        .pump_events(
            &receiver,
            4,
            |_, _| {
                projections += 1;
                Ok(None)
            },
            |_| Ok(()),
        )
        .unwrap();
    assert!(blocked.blocked);
    assert_eq!(projections, 0);
    assert!(service.has_pending());
    consumer.drain(1).unwrap();
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(7),
    };
    let key = SessionKey {
        id: 1,
        generation: 7,
    };
    let mut sequencer = EventSequencer::new(binding, 42);
    let mut properties = Sequences::new(16).unwrap();
    let resumed = service.pump_events(&receiver, 4, |recipient, event| {
        assert_eq!(recipient, EntityId(1));
        assert!(matches!(event, SocialEvent::Chat { accepted, .. } if accepted.text == "Banned account Target."));
        projections += 1;
        project_social_for_session(key, binding, event, &mut sequencer, &mut properties,
            BatchLimits { max_messages: 16, max_bytes: 4096, max_message_bytes: 2048, max_string_bytes: 1024 })
            .map(Some)
    }, Err).unwrap();
    assert!(resumed.blocked);
    assert_eq!(projections, 1);
    assert!(service.has_pending());
    service
        .pump_events(
            &receiver,
            4,
            |_, _| panic!("already projected Audit batch"),
            |_| Ok(()),
        )
        .unwrap();
    assert!(!service.has_pending());
    consumer.drain(1).unwrap();
    let batch = feed.read(FeedChannel::Audit, None, 8).unwrap();
    assert_eq!(
        batch
            .events
            .iter()
            .map(|event| (event.sequence, event.accepted_sequence))
            .collect::<Vec<_>>(),
        [(1, 1), (2, 2)]
    );
    assert_eq!(batch.dropped_publications, 0);
}

#[test]
fn closed_audit_feed_recovers_exact_unpublished_event_and_recipients() {
    let (_feed, publisher, consumer) = ChatFeed::new(ChatApiConfig::default()).unwrap();
    drop(consumer);
    let mut service = SocialService::new(Some(publisher));
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let event = SocialEvent::Chat {
        accepted: AcceptedChat {
            sequence: 17,
            unix_seconds: 123,
            sender: EntityId(1),
            sender_name: "Sentinel".into(),
            channel: ChatChannel::Audit,
            text: "UnBanned account Target.".into(),
        },
        wire: ChatDelivery::LegacyChannel(4),
        recipients: vec![EntityId(1)],
    };
    sender.send(event.clone()).unwrap();
    assert!(
        service
            .pump_events(
                &receiver,
                4,
                |_, _| panic!("Audit recipient must wait"),
                |_| Ok(())
            )
            .unwrap_err()
            .contains("closed")
    );
    assert!(service.has_pending());
    let recovered = service.recover();
    assert_eq!(recovered.event, Some(event));
    assert_eq!(recovered.remaining_recipients, [EntityId(1)]);
    assert!(!recovered.api_attempted);
    assert!(recovered.network.is_none());
}
