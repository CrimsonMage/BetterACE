//! Supervised live, non-durable chat API. Startup fails closed when scoped bot
//! credentials are missing; host/game credentials are never substituted.
use bace_admin::{ChatFeed, ChatPublisher};
use bace_config::ChatApiConfig;
use std::net::SocketAddr;
use tokio::{sync::oneshot, task::JoinHandle};
pub struct ChatApiService {
    publisher: ChatPublisher,
    feed: ChatFeed,
    address: SocketAddr,
    consumer_stop: Option<oneshot::Sender<()>>,
    http_stop: Option<oneshot::Sender<()>>,
    consumer: Option<JoinHandle<Result<(), String>>>,
    http: Option<JoinHandle<Result<(), String>>>,
}
impl ChatApiService {
    pub async fn start(config: ChatApiConfig) -> Result<Option<Self>, String> {
        config.validate().map_err(|e| e.to_string())?;
        if !config.enabled {
            return Ok(None);
        }
        let credentials = config.credentials_file.clone();
        let bots = tokio::task::spawn_blocking(move || bace_admin::load_chat_bots(&credentials))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let listener = tokio::net::TcpListener::bind(config.bind_address)
            .await
            .map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let (feed, publisher, consumer) =
            ChatFeed::new(config).map_err(|e| format!("chat feed: {e:?}"))?;
        let (consumer_stop, stop) = oneshot::channel();
        let consumer = tokio::spawn(async move {
            consumer
                .run_until(stop)
                .await
                .map_err(|e| format!("chat consumer: {e:?}"))
        });
        let (http_stop, stop) = oneshot::channel();
        let http_feed = feed.clone();
        let http = tokio::spawn(async move {
            bace_admin::serve_chat(listener, http_feed, bots, stop)
                .await
                .map_err(|e| e.to_string())
        });
        Ok(Some(Self {
            publisher,
            feed,
            address,
            consumer_stop: Some(consumer_stop),
            http_stop: Some(http_stop),
            consumer: Some(consumer),
            http: Some(http),
        }))
    }
    /// Install this one publication port on the accepted simulation-event drain.
    pub fn publisher(&self) -> ChatPublisher {
        self.publisher.clone()
    }
    pub fn local_address(&self) -> SocketAddr {
        self.address
    }
    pub fn feed(&self) -> &ChatFeed {
        &self.feed
    }
    /// A supervisor must treat early completion of either worker as degraded.
    pub fn healthy(&self) -> bool {
        self.consumer.as_ref().is_some_and(|h| !h.is_finished())
            && self.http.as_ref().is_some_and(|h| !h.is_finished())
    }
    pub async fn shutdown(mut self) -> Result<(), String> {
        if let Some(stop) = self.consumer_stop.take() {
            let _ = stop.send(());
        }
        // Drain accepted publications before withdrawing read access.
        let consumer = self
            .consumer
            .take()
            .ok_or("chat consumer missing")?
            .await
            .map_err(|e| e.to_string())?;
        if let Some(stop) = self.http_stop.take() {
            let _ = stop.send(());
        }
        let mut http = self.http.take().ok_or("chat HTTP worker missing")?;
        let result = match tokio::time::timeout(std::time::Duration::from_secs(5), &mut http).await
        {
            Ok(result) => result.map_err(|e| e.to_string())?,
            Err(_) => {
                http.abort();
                let _ = http.await;
                Err("chat HTTP drain deadline exceeded".into())
            }
        };
        consumer.and(result)
    }
}
impl Drop for ChatApiService {
    fn drop(&mut self) {
        // No detached service survives its supervising runtime. Explicit shutdown
        // performs a graceful bounded drain; cancellation terminates live-only work.
        if let Some(stop) = self.consumer_stop.take() {
            let _ = stop.send(());
        }
        if let Some(stop) = self.http_stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.consumer.take() {
            task.abort();
        }
        if let Some(task) = self.http.take() {
            task.abort();
        }
    }
}
