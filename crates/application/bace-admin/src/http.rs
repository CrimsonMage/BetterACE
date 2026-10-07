use crate::{ControlAction, ControlRequest, HostConsole};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{
        Html, IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use std::{convert::Infallible, future::Future, time::Duration};
use tokio::sync::oneshot;

pub fn console_router(console: HostConsole) -> Router {
    Router::new()
        .route("/", get(|| async { Html(include_str!("ui/index.html")) }))
        .route(
            "/app.js",
            get(|| async {
                (
                    [("content-type", "text/javascript; charset=utf-8")],
                    include_str!("ui/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [("content-type", "text/css; charset=utf-8")],
                    include_str!("ui/style.css"),
                )
            }),
        )
        .route("/api/login", post(login))
        .route("/api/session", get(session))
        .route("/api/logout", post(logout))
        .route("/api/status", get(status))
        .route("/api/control", post(control))
        .route("/api/logs", get(logs))
        .layer(DefaultBodyLimit::max(4096))
        .layer(middleware::from_fn_with_state(console.clone(), security))
        .with_state(console)
}
pub async fn serve_console(
    listener: tokio::net::TcpListener,
    console: HostConsole,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, console_router(console))
        .with_graceful_shutdown(shutdown)
        .await
}
async fn security(
    State(console): State<HostConsole>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let Ok(_permit) = console.inner.requests.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    if !console.authority_valid(request.headers()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = match tokio::time::timeout(Duration::from_secs(10), next.run(request)).await
    {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    for (key, value) in [
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
    ] {
        response
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
    response
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    password: String,
}
async fn login(
    State(console): State<HostConsole>,
    headers: HeaderMap,
    Json(request): Json<Login>,
) -> Response {
    if !console.origin_valid(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !console.admit_login() {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if request.password.is_empty() || request.password.len() > 1024 {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let passwords = console.inner.passwords.clone();
    let hash = console.inner.password.clone();
    let verified =
        tokio::task::spawn_blocking(move || passwords.verify(request.password.as_bytes(), &hash))
            .await;
    if !matches!(verified, Ok(Ok(true))) {
        console.inner.logs.record(
            "warn",
            "host.login_rejected",
            "Host authentication rejected.",
        );
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some((token, csrf)) = console.new_session() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    console
        .inner
        .logs
        .record("info", "host.login", "Host session opened.");
    let cookie = format!(
        "bace_host={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        console.inner.config.session_lifetime_seconds
    );
    ([("set-cookie", cookie)], Json(json!({"csrf":csrf}))).into_response()
}
async fn session(State(console): State<HostConsole>, headers: HeaderMap) -> Response {
    match console.session(&headers, false) {
        Some(csrf) => Json(json!({"csrf":csrf})).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
async fn logout(State(console): State<HostConsole>, headers: HeaderMap) -> Response {
    if console.session(&headers, true).is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    console.logout(&headers);
    (
        [(
            "set-cookie",
            "bace_host=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        )],
        StatusCode::NO_CONTENT,
    )
        .into_response()
}
async fn status(State(console): State<HostConsole>, headers: HeaderMap) -> Response {
    if console.session(&headers, false).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(console.inner.status.borrow().clone()).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlBody {
    action: ControlAction,
}
async fn control(
    State(console): State<HostConsole>,
    headers: HeaderMap,
    Json(body): Json<ControlBody>,
) -> Response {
    if console.session(&headers, true).is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let phase = console.inner.status.borrow().phase.clone();
    if matches!(phase.as_str(), "starting" | "draining" | "stopping") {
        return StatusCode::CONFLICT.into_response();
    }
    let (reply, receive) = oneshot::channel();
    if console
        .inner
        .control
        .try_send(ControlRequest {
            action: body.action,
            reply,
        })
        .is_err()
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    match tokio::time::timeout(Duration::from_secs(2), receive).await {
        Ok(Ok(Ok(id))) => (StatusCode::ACCEPTED, Json(json!({"operation_id":id}))).into_response(),
        Ok(Ok(Err(message))) => (StatusCode::CONFLICT, Json(json!({"error":message}))).into_response(),
        _ => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"Control acknowledgment unavailable; inspect status before retrying."}))).into_response(),
    }
}
#[derive(Deserialize)]
struct LogQuery {
    #[serde(default)]
    after: u64,
}
async fn logs(
    State(console): State<HostConsole>,
    headers: HeaderMap,
    Query(query): Query<LogQuery>,
) -> Response {
    if console.session(&headers, false).is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(permit) = console.inner.streams.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let stream = futures_util::stream::unfold(
        (console, headers, query.after, permit),
        |(console, headers, mut after, permit)| async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            console.session(&headers, false)?;
            let batch = console.inner.logs.recent(after);
            if let Some(last) = batch.records.last() {
                after = last.sequence;
            }
            let event = Event::default()
                .event("logs")
                .id(after.to_string())
                .json_data(batch)
                .ok()?;
            Some((
                Ok::<_, Infallible>(event),
                (console, headers, after, permit),
            ))
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}
