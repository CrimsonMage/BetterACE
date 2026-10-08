use bace_admin::{ChatBotCredential, FeedChannel, provision_chat_bots};
use bace_config::ChatApiConfig;
use bace_gameplay_api::social::{AcceptedChat, ChatChannel};
use bace_runtime::chat_service::ChatApiService;
use bace_types::EntityId;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn pull(address: std::net::SocketAddr, path: &str, token: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    String::from_utf8(response).unwrap()
}
#[tokio::test]
async fn supervised_real_http_uses_scoped_credentials_and_closes_publication_on_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let credentials = directory.path().join("private/bots.toml");
    let bot = ChatBotCredential::generate("bot".into(), vec![FeedChannel::General]).unwrap();
    provision_chat_bots(&credentials, std::slice::from_ref(&bot)).unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let config = ChatApiConfig {
        enabled: true,
        bind_address: address,
        credentials_file: credentials,
        ..Default::default()
    };
    let service = ChatApiService::start(config).await.unwrap().unwrap();
    let publisher = service.publisher();
    let accepted = AcceptedChat {
        sequence: 1,
        unix_seconds: 123,
        sender: EntityId(1),
        sender_name: "Rune".into(),
        channel: ChatChannel::General,
        text: "hello".into(),
    };
    let (events, receiver) = std::sync::mpsc::sync_channel(1);
    events
        .send(bace_gameplay_api::social::SocialEvent::Chat {
            accepted: accepted.clone(),
            wire: bace_gameplay_api::social::ChatDelivery::LegacyChannel(1),
            recipients: vec![],
        })
        .unwrap();
    let mut bridge = bace_runtime::social_service::SocialService::new(Some(publisher.clone()));
    bridge
        .pump_events(&receiver, 4, |_, _| panic!("no recipients"), |_| Ok(()))
        .unwrap();

    for _ in 0..100 {
        if !service
            .feed()
            .read(FeedChannel::General, None, 1)
            .unwrap()
            .events
            .is_empty()
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(service.healthy());
    let response = pull(address, "/api/v1/chat/general", &bot.token).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("hello"));
    assert!(response.contains("no-store"));
    assert!(
        pull(address, "/api/v1/chat/audit", &bot.token)
            .await
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        pull(address, "/api/v1/chat/general", "host-password")
            .await
            .starts_with("HTTP/1.1 403")
    );
    service.shutdown().await.unwrap();
    assert!(publisher.publish(&accepted).is_err());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}
#[tokio::test]
async fn disabled_is_inert_and_enabled_missing_credentials_fails_closed() {
    assert!(
        ChatApiService::start(ChatApiConfig::default())
            .await
            .unwrap()
            .is_none()
    );
    let directory = tempfile::tempdir().unwrap();
    let config = ChatApiConfig {
        enabled: true,
        credentials_file: directory.path().join("missing.toml"),
        ..Default::default()
    };
    assert!(ChatApiService::start(config).await.is_err());
}
