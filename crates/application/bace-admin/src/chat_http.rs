//! Pull-only live API. Audit credentials never imply host control permissions.
use crate::{ChatBotCredential, ChatFeed, FeedChannel, FeedError, HostError, validate_chat_bots};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Semaphore;
#[derive(Clone)]
struct Api {
    feed: ChatFeed,
    bots: Arc<Vec<ChatBotCredential>>,
    requests: Arc<Semaphore>,
}
pub fn chat_router(feed: ChatFeed, bots: Vec<ChatBotCredential>) -> Result<Router, HostError> {
    validate_chat_bots(&bots)?;
    let requests = Arc::new(Semaphore::new(feed.config().max_requests));
    Ok(Router::new()
        .route("/api/v1/chat/{channel}", get(read))
        .with_state(Api {
            feed,
            bots: Arc::new(bots),
            requests,
        }))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    after: Option<String>,
    limit: Option<usize>,
}
async fn read(
    State(api): State<Api>,
    Path(channel): Path<String>,
    Query(query): Query<Request>,
    headers: HeaderMap,
) -> Response {
    let Ok(_permit) = api.requests.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let channel = match channel.as_str() {
        "general" => FeedChannel::General,
        "trade" => FeedChannel::Trade,
        "audit" => FeedChannel::Audit,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    if !token.is_some_and(|token| api.bots.iter().any(|bot| bot.accepts(token, channel))) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = match api.feed.read(
        channel,
        query.after.as_deref(),
        query.limit.unwrap_or(api.feed.config().max_batch),
    ) {
        Ok(batch) => Json(batch).into_response(),
        Err(FeedError::Invalid) => StatusCode::BAD_REQUEST.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// Serve only an already bound loopback listener, using separate read-only bot credentials.
pub async fn serve_chat(
    listener: tokio::net::TcpListener,
    feed: ChatFeed,
    bots: Vec<ChatBotCredential>,
    stop: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), HostError> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(HostError::Permissions);
    }
    let connections = feed.config().max_requests;
    let router = chat_router(feed, bots)?;
    let listener = crate::chat_listener::ChatListener {
        listener,
        capacity: Arc::new(Semaphore::new(connections)),
    };
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = stop.await;
        })
        .await?;
    Ok(())
}
