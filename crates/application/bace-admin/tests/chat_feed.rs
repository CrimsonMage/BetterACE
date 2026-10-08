use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bace_admin::{ChatBotCredential, ChatFeed, FeedChannel, FeedError, chat_router};
use bace_config::ChatApiConfig;
use bace_gameplay_api::social::{AcceptedChat, ChatChannel};
use bace_types::EntityId;
use tower::ServiceExt;
fn event(n: u64, channel: ChatChannel) -> AcceptedChat {
    AcceptedChat {
        sequence: n,
        unix_seconds: 123,
        sender: EntityId(1),
        sender_name: "Player".into(),
        channel,
        text: format!("message{n}"),
    }
}
#[test]
fn bounded_history_cursor_gaps_channel_scope_and_restart() {
    let config = ChatApiConfig {
        events_per_channel: 2,
        publication_capacity: 2,
        ..Default::default()
    };
    let (feed, publisher, mut consumer) = ChatFeed::new(config.clone()).unwrap();
    assert_eq!(
        publisher.publish(&event(1, ChatChannel::Tell)),
        Err(FeedError::PrivateChannel)
    );
    publisher.publish(&event(1, ChatChannel::General)).unwrap();
    consumer.drain(4).unwrap();
    let first = feed.read(FeedChannel::General, None, 1).unwrap();
    assert!(!first.gap);
    publisher.publish(&event(2, ChatChannel::General)).unwrap();
    publisher.publish(&event(3, ChatChannel::General)).unwrap();
    assert_eq!(
        publisher.publish(&event(4, ChatChannel::General)),
        Err(FeedError::Full)
    );
    consumer.drain(4).unwrap();
    publisher.publish(&event(5, ChatChannel::General)).unwrap();
    consumer.drain(4).unwrap();
    let next = feed
        .read(FeedChannel::General, Some(&first.cursor), 128)
        .unwrap();
    assert!(next.gap);
    assert_eq!(next.dropped_publications, 1);
    assert_eq!(
        next.events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        [3, 5]
    );
    assert!(matches!(
        feed.read(FeedChannel::Audit, Some(&first.cursor), 1),
        Err(FeedError::Invalid)
    ));
    let (fresh, _, _) = ChatFeed::new(config).unwrap();
    let reset = fresh
        .read(FeedChannel::General, Some(&first.cursor), 1)
        .unwrap();
    assert!(reset.reset && reset.gap);
}
#[tokio::test]
async fn read_only_api_scopes_do_not_grant_audit_or_control() {
    let (feed, publisher, mut consumer) = ChatFeed::new(Default::default()).unwrap();
    publisher.publish(&event(1, ChatChannel::Audit)).unwrap();
    consumer.drain(10).unwrap();
    let bot =
        ChatBotCredential::generate("general-bot".into(), vec![FeedChannel::General]).unwrap();
    let router = chat_router(feed, vec![bot.clone()]).unwrap();
    let send = |uri: &str, method: &str| {
        Request::builder()
            .uri(uri)
            .method(method)
            .header("authorization", format!("Bearer {}", bot.token))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(send("/api/v1/chat/general", "GET"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .clone()
            .oneshot(send("/api/v1/chat/audit", "GET"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(send("/api/v1/chat/general", "POST"))
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        router
            .oneshot(send("/api/control", "POST"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}
#[test]
fn dropped_tail_is_visible_without_waiting_for_next_chat() {
    let (feed, publisher, mut consumer) = ChatFeed::new(ChatApiConfig {
        publication_capacity: 1,
        ..Default::default()
    })
    .unwrap();
    let cursor = feed.read(FeedChannel::General, None, 1).unwrap().cursor;
    publisher.publish(&event(1, ChatChannel::General)).unwrap();
    assert_eq!(
        publisher.publish(&event(2, ChatChannel::General)),
        Err(FeedError::Full)
    );
    consumer.drain(1).unwrap();
    let result = feed.read(FeedChannel::General, Some(&cursor), 1).unwrap();
    assert!(result.gap);
    assert_eq!(result.dropped_publications, 1);
    assert!(
        !feed
            .read(FeedChannel::General, Some(&result.cursor), 1)
            .unwrap()
            .gap
    );
}

#[test]
fn retained_audit_full_retry_has_one_contiguous_sequence_and_no_drop() {
    let (feed, publisher, mut consumer) = ChatFeed::new(ChatApiConfig {
        publication_capacity: 1,
        ..Default::default()
    })
    .unwrap();
    publisher
        .try_publish_audit(&event(1, ChatChannel::Audit))
        .unwrap();
    assert_eq!(
        publisher.try_publish_audit(&event(2, ChatChannel::Audit)),
        Err(FeedError::Full)
    );
    consumer.drain(1).unwrap();
    publisher
        .try_publish_audit(&event(2, ChatChannel::Audit))
        .unwrap();
    consumer.drain(1).unwrap();
    let batch = feed.read(FeedChannel::Audit, None, 8).unwrap();
    assert_eq!(
        batch
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(batch.dropped_publications, 0);
    assert!(!batch.gap);
}
