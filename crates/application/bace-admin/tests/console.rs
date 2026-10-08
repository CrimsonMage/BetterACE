#![cfg(not(windows))]
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use bace_admin::{
    ContentAction, ContentChange, ContentPreview, ContentReply, ContentRequest, ContentRevision,
    HostConsole, HostStatus, console_router, load_operator, provision_operator,
};
use bace_config::HostConfig;
use bace_observability::LogStore;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tower::ServiceExt;

fn fixture() -> (tempfile::TempDir, Router) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private/operator.toml");
    provision_operator(&path, b"host-test-password").unwrap();
    let config = HostConfig {
        max_log_streams: 1,
        ..HostConfig::default()
    };
    let logs = Arc::new(LogStore::open(&directory.path().join("logs")).unwrap());
    let (_, status) = watch::channel(HostStatus::default());
    let (control, _) = mpsc::channel(1);
    let console =
        HostConsole::new(config, load_operator(&path).unwrap(), logs, status, control).unwrap();
    (directory, console_router(console))
}
fn content_fixture() -> (tempfile::TempDir, Router, mpsc::Receiver<ContentRequest>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private/operator.toml");
    provision_operator(&path, b"host-test-password").unwrap();
    let logs = Arc::new(LogStore::open(&directory.path().join("logs")).unwrap());
    let (_, status) = watch::channel(HostStatus::default());
    let (control, _) = mpsc::channel(1);
    let (content, requests) = mpsc::channel(1);
    let console = HostConsole::new_with_content(
        HostConfig::default(),
        load_operator(&path).unwrap(),
        logs,
        status,
        control,
        Some(content),
    )
    .unwrap();
    (directory, console_router(console), requests)
}
fn request(method: &str, path: &str, cookie: &str, csrf: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("origin", "http://127.0.0.1:8080")
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}
async fn login(router: &Router) -> (String, String) {
    let response = router
        .clone()
        .oneshot(request(
            "POST",
            "/api/login",
            "",
            "",
            r#"{"password":"host-test-password"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, parsed["csrf"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn host_auth_csrf_origin_and_logout_are_enforced() {
    let (_directory, router) = fixture();
    let response = router
        .clone()
        .oneshot(request("GET", "/api/status", "", "", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let mut wrong_host = request("GET", "/", "", "", "");
    wrong_host
        .headers_mut()
        .insert("host", "evil.example".parse().unwrap());
    assert_eq!(
        router.clone().oneshot(wrong_host).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut wrong_origin = request(
        "POST",
        "/api/login",
        "",
        "",
        r#"{"password":"host-test-password"}"#,
    );
    wrong_origin
        .headers_mut()
        .insert("origin", "https://evil.example".parse().unwrap());
    assert_eq!(
        router.clone().oneshot(wrong_origin).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let (cookie, csrf) = login(&router).await;
    assert_eq!(
        router
            .clone()
            .oneshot(request("GET", "/api/status", &cookie, "", ""))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(
                "POST",
                "/api/control",
                &cookie,
                "wrong",
                r#"{"action":"restart"}"#
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("POST", "/api/logout", &cookie, &csrf, "{}"))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        router
            .oneshot(request("GET", "/api/status", &cookie, "", ""))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn stream_capacity_and_request_body_are_bounded() {
    let (_directory, router) = fixture();
    let (cookie, _) = login(&router).await;
    let first = router
        .clone()
        .oneshot(request("GET", "/api/logs", &cookie, "", ""))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let second = router
        .clone()
        .oneshot(request("GET", "/api/logs", &cookie, "", ""))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    drop(first);
    assert_eq!(
        router
            .clone()
            .oneshot(request("GET", "/api/logs", &cookie, "", ""))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let large = format!(r#"{{"password":"{}"}}"#, "x".repeat(5000));
    assert_eq!(
        router
            .oneshot(request("POST", "/api/login", "", "", &large))
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn content_review_requires_host_csrf_and_publish_token() {
    let (_directory, router, mut requests) = content_fixture();
    let (cookie, csrf) = login(&router).await;
    assert_eq!(
        router
            .clone()
            .oneshot(request(
                "POST",
                "/api/content/preview",
                &cookie,
                "wrong",
                "{}"
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(
                "POST",
                "/api/content/publish",
                &cookie,
                &csrf,
                r#"{"token":"bad"}"#
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let reviewing = tokio::spawn({
        let router = router.clone();
        let cookie = cookie.clone();
        let csrf = csrf.clone();
        async move {
            router
                .oneshot(request(
                    "POST",
                    "/api/content/preview",
                    &cookie,
                    &csrf,
                    "{}",
                ))
                .await
                .unwrap()
        }
    });
    let incoming = requests.recv().await.unwrap();
    assert!(matches!(incoming.action, ContentAction::Preview));
    assert!(
        incoming
            .reply
            .send(Ok(ContentReply::Preview(ContentPreview {
                token: "a".repeat(64),
                total: 1,
                entries: vec![ContentChange {
                    path: "world/quest.toml".into(),
                    kind: "quest".into(),
                    id: 8,
                    name: "The Quest".into(),
                    action: "add".into()
                }],
            })))
            .is_ok()
    );
    let response = reviewing.await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["entries"][0]["kind"], "quest");
    let staging = tokio::spawn({
        let router = router.clone();
        let cookie = cookie.clone();
        let csrf = csrf.clone();
        async move {
            router
                .oneshot(request(
                    "POST",
                    "/api/content/removal",
                    &cookie,
                    &csrf,
                    r#"{"kind":"quest","id":8}"#,
                ))
                .await
                .unwrap()
        }
    });
    let incoming = requests.recv().await.unwrap();
    assert!(
        matches!(incoming.action, ContentAction::StageRemoval { ref kind, id: 8 } if kind == "quest")
    );
    assert!(
        incoming
            .reply
            .send(Ok(ContentReply::Staged {
                path: "removals/example.toml".into()
            }))
            .is_ok()
    );
    assert_eq!(staging.await.unwrap().status(), StatusCode::ACCEPTED);
    let reading = tokio::spawn({
        let router = router.clone();
        let cookie = cookie.clone();
        async move {
            router
                .oneshot(request(
                    "GET",
                    "/api/content/revision?revision=42",
                    &cookie,
                    "",
                    "",
                ))
                .await
                .unwrap()
        }
    });
    let incoming = requests.recv().await.unwrap();
    assert!(matches!(
        incoming.action,
        ContentAction::Revision { revision: 42 }
    ));
    assert!(
        incoming
            .reply
            .send(Ok(ContentReply::Revision(ContentRevision {
                revision: 42,
                status: "accepted".into(),
                rejection: None,
            })))
            .is_ok()
    );
    let response = reading.await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn provisioning_never_overwrites_and_checks_private_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private/operator.toml");
    provision_operator(&path, b"first-password").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(provision_operator(&path, b"second-password").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_operator(&path).is_err());
    }
}
