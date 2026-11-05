//! Fixed retained production lifecycle, independent of authenticated host I/O.
use super::*;
use crate::startup_trace::StartupTrace;
use crate::{
    game_bootstrap::{GameBootstrap, GameClock},
    game_runtime::{GameRuntime, GameRuntimeLimits, GameRuntimeShutdown, GameRuntimeStartup},
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
type Prepare = Pin<Box<dyn Future<Output = Result<GameRuntimeStartup, String>>>>;
pub(super) async fn run(
    config: bace_config::ServerConfig,
    mut controls: mpsc::Receiver<u64>,
    publish: watch::Sender<Status>,
    lost: Arc<AtomicBool>,
) {
    let log_directory = config.host.log_directory.join("game");
    let log = match tokio::task::spawn_blocking(move || {
        bace_observability::LogStore::open(&log_directory)
    })
    .await
    {
        Ok(Ok(log)) => Arc::new(log),
        _ => {
            publish.send_replace(Status {
                ready: false,
                drained: true,
                detail: "Game diagnostics could not start; no world was opened.".into(),
            });
            return;
        }
    };
    let trace = StartupTrace::new(log.clone());
    let mut preparing: Option<Prepare> = Some(Box::pin(prepare(config.clone(), trace.clone())));
    let mut startup: Option<GameRuntimeStartup> = None;
    let mut game: Option<Box<GameRuntime>> = None;
    let mut shutdown: Option<GameRuntimeShutdown> = None;
    let mut drain = None;
    let mut closed = false;
    let mut failure = None::<String>;
    let mut logged_failure = None::<String>;
    let mut running_after = None::<f64>;
    let mut running_resources = None::<String>;
    let mut first = true;
    let mut ownership_check = tokio::time::Instant::now();
    let mut turns = tokio::time::interval(Duration::from_millis(5));
    turns.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        turns.tick().await;
        while let Ok(operation) = controls.try_recv() {
            drain.get_or_insert(operation);
        }
        if controls.is_closed() {
            lost.store(true, Ordering::Release);
        }
        if let Some(job) = preparing.as_mut()
            && let Poll::Ready(result) = job.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        {
            preparing = None;
            match result {
                Ok(owner) => startup = Some(owner),
                Err(error) => {
                    failure = Some(error);
                    closed = true;
                }
            }
        }
        if let Some(owner) = startup.as_mut() {
            let stage = owner.stage();
            let _step = failure.is_none().then(|| trace.step(stage));
            if failure.is_none() {
                publish.send_replace(Status {
                    ready: false,
                    drained: false,
                    detail: format!("Preparing: {stage}."),
                });
            }
            match owner.advance().await {
                Ok(Some(mut runtime)) => {
                    runtime.attach_diagnostic_log(log.clone());
                    failure = runtime.attach_staff_broadcast_log(log.clone()).err();
                    game = Some(runtime);
                    startup = None;
                    running_after = Some(trace.elapsed_seconds());
                    running_resources = Some(crate::startup_trace::resource_summary());
                }
                Ok(None) => {}
                Err(error) => failure = Some(error),
            }
        }
        if let Some(runtime) = game.as_mut() {
            if tokio::time::Instant::now() >= ownership_check {
                ownership_check = tokio::time::Instant::now() + Duration::from_secs(5);
                if let Err(error) = runtime.verify_world_owner().await {
                    failure = Some(error);
                    lost.store(true, Ordering::Release);
                }
            }
            if (drain.is_some() || lost.load(Ordering::Acquire))
                && let Err(error) = runtime.quiesce_now()
            {
                failure = Some(error);
            }
            if let Err(error) = runtime.poll_now() {
                failure = Some(error);
            }
            if let Err(error) = log_shard(runtime, &log) {
                failure = Some(error);
            }
            if runtime.shard_world_stop_requested() {
                drain.get_or_insert(0);
            }
            if drain.is_some() && !runtime.requires_drain() {
                let runtime = game.take().expect("retained game owner");
                match runtime.into_shutdown() {
                    Ok(owner) => shutdown = Some(owner),
                    Err((error, owner)) => {
                        game = Some(owner);
                        failure = Some(error);
                    }
                }
            }
        }
        if let Some(owner) = shutdown.as_mut() {
            match owner.poll() {
                Ok(true) => {
                    closed = true;
                    failure = None;
                }
                Ok(false) => {}
                Err(error) => failure = Some(error),
            }
        }
        if failure != logged_failure {
            if let Some(error) = &failure {
                let message: String = error
                    .chars()
                    .take(512)
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                eprintln!("Game lifecycle blocked; retained owner: {message}");
                log.record("error", "game.lifecycle_blocked", &message);
            }
            logged_failure = failure.clone();
        }
        let ready = game.as_ref().is_some_and(|g| !g.status().quiescing) && failure.is_none();
        let detail = if let Some(error) = &failure {
            format!("Game lifecycle blocked; retained owner: {error}")
        } else if closed {
            "Durable game shutdown complete; world lease released.".into()
        } else if drain.is_some() || lost.load(Ordering::Acquire) {
            match game.as_ref() {
                Some(runtime) => format!(
                    "Draining admitted players, world state and reliable output: {}.",
                    runtime.pending_drain_reason()
                ),
                None => "Draining final durable shutdown work.".into(),
            }
        } else if ready {
            format!(
                "Game services running after {:.3}s; {}. Stock-client playability remains unqualified.",
                running_after.unwrap_or_else(|| trace.elapsed_seconds()),
                running_resources
                    .as_deref()
                    .unwrap_or("resource counters pending")
            )
        } else {
            format!("Preparing: {}.", trace.current())
        };
        if first
            || publish.borrow().detail != detail
            || publish.borrow().ready != ready
            || publish.borrow().drained != closed
        {
            publish.send_replace(Status {
                ready,
                drained: closed,
                detail,
            });
            first = false;
        }
        if closed {
            return;
        }
    }
}
fn log_shard(runtime: &mut GameRuntime, log: &bace_observability::LogStore) -> Result<(), String> {
    for _ in 0..32 {
        let Some(record) = runtime.pending_shard_host_record() else {
            break;
        };
        let (event, text) = match &record.event.effect {
            crate::shard_control::ShardEffect::Reply { text, .. } => ("shard.reply", text),
            crate::shard_control::ShardEffect::Broadcast { text } => ("shard.broadcast", text),
            crate::shard_control::ShardEffect::Audit { text, .. } => ("shard.audit", text),
            crate::shard_control::ShardEffect::Log { text } => ("shard.log", text),
        };
        let sequence = record.event.sequence;
        match log.try_record_exact(bace_observability::LogRecord {
            sequence: 0,
            unix_millis: u64::try_from(record.unix_millis)
                .map_err(|_| "negative shard log timestamp")?,
            level: "INFO".into(),
            event: event.into(),
            message: text.clone(),
        }) {
            Ok(()) => runtime.acknowledge_shard_host_record(sequence)?,
            Err(error) if error.kind == bace_observability::ExactLogErrorKind::Full => break,
            Err(error) => return Err(format!("shard log handoff retained: {:?}", error.kind)),
        }
    }
    Ok(())
}
async fn prepare(
    config: bace_config::ServerConfig,
    trace: StartupTrace,
) -> Result<GameRuntimeStartup, String> {
    let treasure_table_set_id = config.treasure_table_set_id;
    let limits = GameRuntimeLimits {
        sessions: config.max_sessions,
        loading: 4,
        messages: 1024,
        message_bytes: 64 * 1024 * 1024,
        work_per_poll: 64,
    };
    let mut bootstrap = GameBootstrap::prepare(config, &trace).await?;
    let manifest = bootstrap.assets.clone();
    let generation = bootstrap.pack.generation.clone();
    let world_index = bootstrap.world_index.take();
    let asset_trace = trace.clone();
    let mut assets = tokio::task::spawn_blocking(move || {
        let _step = asset_trace.step("attach approved region DAT indexes");
        let mut verified = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
        drop(_step);
        let _step = asset_trace.step("prepare spells, treasure and startup assets");
        verified.prepare_runtime_startup_assets_traced(
            &generation,
            crate::startup_assets::StartupAssetPolicy {
                treasure_table_set_id,
                ..Default::default()
            },
            Some(&asset_trace),
            world_index,
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    assets.generators.drop_plain_wield =
        bootstrap.config.world.death.creatures_drop_createlist_wield;
    let clock = GameClock::capture()?;
    GameRuntimeStartup::new(bootstrap, assets, limits, clock).map_err(|(error, _)| error)
}
