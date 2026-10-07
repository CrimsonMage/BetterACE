use crate::{PublicationError, PublicationResult, load_catalog, publish_pending_once};
use bace_config::ServerConfig;
use bace_db_postgres::{PgStore, StoreError};
use std::{future::Future, time::Duration};
use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("{0} must contain the PostgreSQL URL")]
    MissingUrl(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Publication(#[from] PublicationError),
    #[error("shutdown signal failed: {0}")]
    Signal(#[from] std::io::Error),
    #[error("content worker {0} deadline exceeded; restart from durable accepted heads")]
    Deadline(&'static str),
    #[error("content consumer failed: {0}")]
    Consumer(#[from] tokio::task::JoinError),
}

pub async fn run(config: ServerConfig) -> Result<(), WorkerError> {
    let url = std::env::var(&config.database_url_env)
        .map_err(|_| WorkerError::MissingUrl(config.database_url_env.clone()))?;
    run_until(
        &url,
        config.database_connections,
        Duration::from_secs(30),
        Duration::from_secs(5),
        tokio::signal::ctrl_c(),
    )
    .await
}

/// Cancellation exits this worker. It must never resume an old catalog after an uncertain commit.
async fn run_until(
    url: &str,
    connections: u32,
    operation_timeout: Duration,
    shutdown_timeout: Duration,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
) -> Result<(), WorkerError> {
    tokio::pin!(shutdown);
    let store = tokio::select! {
        biased;
        signal = &mut shutdown => return signal.map_err(WorkerError::Signal),
        connected = tokio::time::timeout(operation_timeout, PgStore::connect_bounded(url, connections, operation_timeout)) => {
            connected.map_err(|_| WorkerError::Deadline("connect"))??
        }
    };
    let result = async {
        let mut catalog = tokio::select! {
            biased;
            signal = &mut shutdown => return signal.map_err(WorkerError::Signal),
            loaded = tokio::time::timeout(operation_timeout, async {
                store.migrate().await?;
                load_catalog(&store).await.map_err(WorkerError::Publication)
            }) => loaded.map_err(|_| WorkerError::Deadline("startup"))??,
        };
        let (send, mut receive) = mpsc::channel(1);
        let mut consumer = tokio::spawn(async move {
            let mut ticks = tokio::time::interval(Duration::from_nanos(1_000_000_000 / 30));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticks.tick().await;
                match receive.try_recv() {
                    // Harness only: the future live world will own this Arc at a tick boundary.
                    Ok(snapshot) => drop(snapshot),
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }
            }
        });
        let mut poll = tokio::time::interval(Duration::from_millis(250));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        println!("Content worker active; polling durable publications. No game sockets opened.");
        let result = loop {
            // Keep shutdown selectable throughout validation, delivery backpressure and COMMIT.
            tokio::select! {
                biased;
                signal = &mut shutdown => break signal.map_err(WorkerError::Signal),
                published = async {
                    poll.tick().await;
                    tokio::time::timeout(operation_timeout, publish_pending_once(&store, &mut catalog, &send)).await
                } => match published {
                    Ok(Ok(PublicationResult::Idle)) => {}
                    Ok(Ok(outcome)) => println!("{outcome:?}"),
                    Ok(Err(error)) => break Err(WorkerError::Publication(error)),
                    Err(_) => break Err(WorkerError::Deadline("publication")),
                }
            }
        };
        drop(send);
        match tokio::time::timeout(shutdown_timeout, &mut consumer).await {
            Ok(joined) => joined?,
            Err(_) => {
                consumer.abort();
                return Err(WorkerError::Deadline("consumer shutdown"));
            }
        }
        result
    }
    .await;
    // Dropped SQL futures can still have backend work in progress; server deadlines bound it too.
    tokio::time::timeout(shutdown_timeout, store.close())
        .await
        .map_err(|_| WorkerError::Deadline("pool shutdown"))?;
    result
}

#[cfg(test)]
mod tests;
