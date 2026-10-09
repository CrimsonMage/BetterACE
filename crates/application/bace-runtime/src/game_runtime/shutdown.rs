//! A fixed retained shutdown future. Timeouts cancel a borrow/poll, never the
//! world owner, recovered kernel, save drain or an uncertain shutdown result.
use super::*;
#[cfg(test)]
mod tests;
#[derive(Default)]
struct Recovery {
    _shard: Option<shard::ShardRuntime>,
    _simulation: Option<Box<crate::simulation::SimulationExit>>,
    _simulation_error: Option<crate::simulation::WorkerError>,
    _simulation_handle: Option<Rc<SimulationWorker>>,
    _generator: Option<Box<GeneratorService>>,
    _region: Option<Box<RegionService>>,
    _cold: Option<Box<PlayerPreparationWorker>>,
    _authentication: Option<crate::authentication_pool::AuthenticationDrain>,
    _dat: Option<ddd::DatRuntime>,
    _dat_drain: Option<crate::dat_distribution::DatPreparationDrain>,
    _social: Vec<crate::social_lookup::PendingSocialAction>,
    _pack: Vec<crate::pack_io::PackCompletion>,
}
struct Finished {
    _bootstrap: Option<GameBootstrap>,
    _recovery: Recovery,
    failure: Option<String>,
}
#[must_use = "retain shutdown until every owner drains and the world lease closes"]
pub struct GameRuntimeShutdown {
    runtime: Option<Box<GameRuntime>>,
    threads: Vec<std::thread::JoinHandle<()>>,
    job: Option<Job<Finished>>,
    finished: Option<Finished>,
    failure: Option<String>,
}
impl GameRuntime {
    pub fn into_shutdown(self: Box<Self>) -> Result<GameRuntimeShutdown, (String, Box<Self>)> {
        if !self.draining || self.requires_drain() || Rc::strong_count(&self.simulation) != 1 {
            return Err((
                "gameplay and durable output must drain before worker shutdown".into(),
                self,
            ));
        }
        Ok(GameRuntimeShutdown {
            runtime: Some(self),
            threads: vec![],
            job: None,
            finished: None,
            failure: None,
        })
    }
}
impl GameRuntimeShutdown {
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub fn complete(&self) -> bool {
        self.finished.as_ref().is_some_and(|f| f.failure.is_none())
    }
    pub fn poll(&mut self) -> Result<bool, String> {
        let result = self.poll_inner();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn poll_inner(&mut self) -> Result<bool, String> {
        if let Some(finished) = &self.finished {
            return finished.failure.clone().map_or(Ok(true), Err);
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.shutdown_staff_workers()?;
            self.threads.extend(runtime.shutdown_magic_workers()?);
            self.threads.extend(runtime.shutdown_npc_workers()?);
            self.threads.extend(runtime.shutdown_creation_worker()?);
            let runtime = self.runtime.take().expect("retained shutdown runtime");
            let threads = std::mem::take(&mut self.threads);
            self.job = Some(Box::pin(stop(runtime, threads)));
        }
        if let Some(job) = self.job.as_mut()
            && let Poll::Ready(finished) =
                job.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        {
            self.job = None;
            self.failure = finished.failure.clone();
            self.finished = Some(finished);
        }
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        Ok(self.complete())
    }
}
async fn stop(runtime: Box<GameRuntime>, threads: Vec<std::thread::JoinHandle<()>>) -> Finished {
    let GameRuntime {
        mut bootstrap,
        simulation,
        network,
        saves,
        authentication,
        world,
        preparation,
        dat,
        social,
        mut shard,
        ..
    } = *runtime;
    let mut recovery = Recovery::default();
    let mut failures = Vec::new();
    if let Some(dat) = dat {
        match tokio::task::spawn_blocking(move || dat.shutdown()).await {
            Ok(Ok(())) => {}
            Ok(Err((owner, drain))) => {
                recovery._dat = Some(owner);
                recovery._dat_drain = drain;
                failures.push("DAT preparation shutdown retained recovery".into());
            }
            Err(_) => failures.push("DAT preparation shutdown task failed".into()),
        }
    }
    let joined = tokio::task::spawn_blocking(move || {
        let mut succeeded = true;
        for thread in threads {
            succeeded &= thread.join().is_ok();
        }
        succeeded
    })
    .await;
    if !matches!(joined, Ok(true)) {
        failures.push("cold worker shutdown failed".to_owned());
    }
    match Rc::try_unwrap(simulation) {
        Err(owner) => {
            recovery._simulation_handle = Some(owner);
            failures.push("simulation still has adapter owners".into());
        }
        Ok(worker) => match tokio::task::spawn_blocking(move || worker.shutdown_recover()).await {
            Ok(Ok(mut exit)) => {
                if let Err(error) = exit.prepare_durable_shutdown() {
                    failures.push(error);
                    recovery._simulation = Some(Box::new(exit));
                }
            }
            Ok(Err(error)) => {
                recovery._simulation_error = Some(error);
                failures.push("simulation stop failed; recovery retained".into());
            }
            Err(_) => failures.push("simulation join task failed".into()),
        },
    }
    // No service below can acknowledge gameplay: the online/durable owner
    // preflight already required zero pending transactions and offline leases.
    saves.handle.close();
    match saves.task.await {
        Ok(summary) if !summary.shutdown_timed_out => {}
        _ => failures.push("save worker did not confirm close".into()),
    }
    let auth = authentication.shutdown().await;
    if !auth.completions.is_empty()
        || !auth.unrecovered_sessions.is_empty()
        || auth.panicked_workers != 0
        || !auth.passwords_drained
    {
        recovery._authentication = Some(auth);
        failures.push("authentication completion retained".into());
    }
    if !matches!(
        tokio::task::spawn_blocking(move || network.shutdown()).await,
        Ok(Ok(()))
    ) {
        failures.push("network close failed".into());
    }
    match preparation.try_shutdown() {
        Ok(thread) => {
            if !matches!(
                tokio::task::spawn_blocking(move || thread.join()).await,
                Ok(Ok(()))
            ) {
                failures.push("player cold worker join failed".into());
            }
        }
        Err(worker) => {
            recovery._cold = Some(worker);
            failures.push("player preparation retained work".into());
        }
    }
    if let Some(world) = world {
        let result = tokio::task::spawn_blocking(move || {
            (world.generators.shutdown(), world.regions.shutdown())
        })
        .await;
        match result {
            Ok((generator, region)) => {
                match generator {
                    Ok(_) => {}
                    Err(crate::generator_service::GeneratorShutdownError::Pending(owner)) => {
                        recovery._generator = Some(owner);
                        failures.push("generator owner retained work".into());
                    }
                    Err(crate::generator_service::GeneratorShutdownError::Worker(error)) => {
                        failures.push(error)
                    }
                }
                match region {
                    Ok(()) => {}
                    Err(crate::region_service::RegionServiceShutdownError::Pending(owner)) => {
                        recovery._region = Some(owner);
                        failures.push("region owner retained work".into());
                    }
                    Err(crate::region_service::RegionServiceShutdownError::Worker(error)) => {
                        failures.push(error)
                    }
                }
            }
            Err(_) => failures.push("region/generator join task failed".into()),
        }
    } else {
        failures.push("world service owner missing".into());
    }
    let (api, lookup) = social.into_shutdown();
    match tokio::task::spawn_blocking(move || lookup.stop()).await {
        Ok((pending, result)) => {
            if !pending.is_empty() {
                recovery._social = pending;
                failures.push("social actions retained".into());
            }
            if let Err(error) = result {
                failures.push(error);
            }
        }
        Err(_) => failures.push("social lookup join failed".into()),
    }
    if let Some(api) = api
        && let Err(error) = api.shutdown().await
    {
        failures.push(error);
    }
    if let Some(worker) = bootstrap.pack_io.take() {
        match tokio::task::spawn_blocking(move || worker.shutdown()).await {
            Ok(Ok(rows)) if rows.is_empty() => {}
            Ok(Ok(rows)) => {
                recovery._pack = rows;
                failures.push("pack receipts retained".into());
            }
            Ok(Err(error)) => failures.push(error),
            Err(_) => failures.push("pack worker join failed".into()),
        }
    } else {
        failures.push("pack owner missing".into());
    }
    if !failures.is_empty() {
        recovery._shard = Some(shard);
        return Finished {
            _bootstrap: Some(bootstrap),
            _recovery: recovery,
            failure: Some(failures.join("; ")),
        };
    }
    let mut failure = bootstrap
        .world_owner
        .close()
        .await
        .err()
        .map(|e| format!("world lease close: {e}"));
    if failure.is_none() {
        failure = shard.finish_world_stop().err();
    }
    if failure.is_some() {
        recovery._shard = Some(shard);
    }
    Finished {
        _bootstrap: None,
        _recovery: recovery,
        failure,
    }
}
