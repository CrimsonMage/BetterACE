//! Retained startup owner. Immutable source closures are supplied by the verified
//! asset-preparation lane; no synthetic scene or guessed gameplay asset is added.
use super::*;
use crate::{
    generator_preparation::{CreatureAdmissionPolicy, GeneratorPreparationOptions},
    generator_service::GeneratorServiceConfig,
    region_service::RegionServiceConfig,
    simulation::{SimulationConfig, WorkerError},
};

type NpcInventoryIdentitySets = (
    Arc<std::collections::BTreeSet<bace_types::EntityId>>,
    Arc<std::collections::BTreeSet<bace_types::EntityId>>,
);

pub struct GameRuntimeStartupAssets {
    pub recall_locations: Arc<bace_simulation::PreparedRecallLocations>,
    pub runtime: GameRuntimeAssets,
    pub staff_magic: Arc<crate::staff_magic_assets::PreparedStaffMagicAssets>,
    pub levels: Arc<bace_character::CharacterLevelTable>,
    pub treasure: Arc<bace_loot::TreasureAssets>,
    pub creatures: CreatureAdmissionPolicy,
    pub generators: GeneratorPreparationOptions,
    pub aetheria_drop_rate: f32,
}
/// Keep this value on the adapter executor across retries and cancelled awaits.
/// Until `advance` returns a complete runtime it retains the exclusive database
/// owner and every service already constructed, including a rejected kernel.
#[must_use = "startup retains the database world owner and any started services"]
pub struct GameRuntimeStartup {
    bootstrap: Option<GameBootstrap>,
    assets: GameRuntimeStartupAssets,
    limits: GameRuntimeLimits,
    clock: GameClock,
    kernel: Option<bace_simulation::Kernel>,
    kernel_configured: bool,
    chat: Option<crate::chat_service::ChatApiService>,
    chat_started: bool,
    simulation: Option<SimulationWorker>,
    simulation_failure: Option<WorkerError>,
    network: Option<NetworkThread>,
    saves: Option<SaveWorker>,
    authentication: Option<AuthenticationPool>,
    players: Option<PlayerService>,
    online: Option<OnlinePlayerSaveService>,
    regions: Option<RegionService>,
    npc_inventory: Option<NpcInventoryIdentitySets>,
    generators: Option<GeneratorService>,
    preparation: Option<PlayerPreparationWorker>,
    rejected: Option<GameRuntimeOwners>,
    failure: Option<String>,
}
impl GameRuntimeStartup {
    pub fn new(
        bootstrap: GameBootstrap,
        assets: GameRuntimeStartupAssets,
        limits: GameRuntimeLimits,
        clock: GameClock,
    ) -> Result<Self, (String, Box<GameBootstrap>)> {
        let valid = limits.validate().and_then(|()| {
            if limits.sessions != bootstrap.config.max_sessions
                || assets.staff_magic.definitions.as_slice()
                    != assets.runtime.staff_definitions.as_ref()
                || !assets.aetheria_drop_rate.is_finite()
                || assets.aetheria_drop_rate < 0.
                || assets.creatures.corpse_template == 0
                || assets.creatures.think_interval == 0
                || assets.creatures.corpse_decay_ticks == 0
            {
                return Err("startup source policy/capacity mismatch".into());
            }
            Ok(())
        });
        if let Err(error) = valid {
            return Err((error, Box::new(bootstrap)));
        }
        Ok(Self {
            bootstrap: Some(bootstrap),
            assets,
            limits,
            clock,
            kernel: None,
            kernel_configured: false,
            chat: None,
            chat_started: false,
            simulation: None,
            simulation_failure: None,
            network: None,
            saves: None,
            authentication: None,
            players: None,
            online: None,
            regions: None,
            npc_inventory: None,
            generators: None,
            preparation: None,
            rejected: None,
            failure: None,
        })
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    /// Construct at most one owner per call. Async I/O is restricted to opening
    /// the reserved save writer and optional HTTP service. The caller may cancel
    /// this borrow without dropping the retained startup state.
    pub async fn advance(&mut self) -> Result<Option<Box<GameRuntime>>, String> {
        // Cold startup returns sizeable owner aggregates. Keep the nested
        // future on the heap rather than stacking its move/drop frames on a
        // standard 2 MiB adapter thread. This allocation occurs only at startup.
        let result = Box::pin(self.advance_inner()).await;
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    async fn advance_inner(&mut self) -> Result<Option<Box<GameRuntime>>, String> {
        if self.simulation_failure.is_some() {
            return Err("simulation failure retains its original owner".into());
        }
        if let Some(owners) = self.rejected.take() {
            return self.finish(owners);
        }
        let bootstrap = self.bootstrap.as_ref().ok_or("startup already completed")?;
        if self.kernel.is_none() && self.simulation.is_none() {
            self.kernel = Some(
                bace_simulation::Kernel::new(
                    bace_world::World::default(),
                    bootstrap.config.command_capacity,
                )
                .map_err(|e| format!("kernel startup: {e:?}"))?,
            );
            return Ok(None);
        }
        if !self.kernel_configured {
            // Configuration may partially install immutable source state. Retain
            // any rejected kernel and require explicit inspection, never replay
            // configuration against a half-configured owner.
            if self.failure.is_some() {
                return Err("kernel configuration failed; original owner retained".into());
            }
            self.kernel
                .as_mut()
                .ok_or("startup kernel missing")?
                .seed_npc_ticket_epoch(bootstrap.world_owner.epoch())
                .map_err(|e| format!("NPC ticket epoch: {e:?}"))?;
            bootstrap.configure_kernel(
                self.kernel.as_mut().ok_or("startup kernel missing")?,
                self.clock,
                self.assets.levels.clone(),
            )?;
            let staff = &self.assets.staff_magic;
            self.kernel
                .as_mut()
                .ok_or("startup kernel missing")?
                .register_staff_magic_assets(
                    staff.definitions.clone(),
                    staff.plans.clone(),
                    staff.enchantments.clone(),
                )
                .map_err(|error| format!("staff magic startup: {error:?}"))?;
            self.kernel
                .as_mut()
                .ok_or("startup kernel missing")?
                .register_recall_locations(self.assets.recall_locations.clone())
                .map_err(|error| format!("recall locations startup: {error:?}"))?;
            self.kernel_configured = true;
            return Ok(None);
        }
        if self.players.is_none() {
            self.players = Some(PlayerService::new(
                crate::game_login::GameLoginService::new(
                    bootstrap.store.clone(),
                    self.limits.sessions,
                    11,
                )
                .map_err(|e| e.to_string())?,
                self.limits.sessions,
            )?);
            self.online = Some(OnlinePlayerSaveService::new(
                self.limits.sessions,
                64 * 1024 * 1024,
                tokio::time::Instant::now(),
            )?);
            return Ok(None);
        }
        if self.preparation.is_none() {
            self.preparation = Some(PlayerPreparationWorker::start(
                bootstrap.assets.clone(),
                self.limits.loading,
            )?);
            return Ok(None);
        }
        if self.npc_inventory.is_none() {
            let pending =
                crate::npc_recovery::pending_source_inventory(&bootstrap.store, 65536).await?;
            let archived =
                crate::npc_recovery::archived_source_inventory(&bootstrap.store, 65536).await?;
            self.npc_inventory = Some((Arc::new(pending), Arc::new(archived)));
            return Ok(None);
        }
        if self.regions.is_none() {
            self.regions = Some(RegionService::start(
                bootstrap.store.clone(),
                RegionServiceConfig {
                    world_epoch: bootstrap.world_owner.epoch(),
                    capacity: bootstrap.world_settings.max_resident_regions,
                    assets: bootstrap.assets.clone(),
                    generation: bootstrap.pack.generation.clone(),
                    creature_policy: self.assets.creatures,
                    treasure_assets: self.assets.treasure.clone(),
                    random: bootstrap.random.root(),
                    content_hash: bootstrap
                        .pack
                        .manifest
                        .content_hash(Default::default())
                        .map_err(|e| e.to_string())?,
                    aetheria_drop_rate: self.assets.aetheria_drop_rate,
                    options: self.assets.generators,
                },
            )?);
            self.regions
                .as_mut()
                .expect("started regions")
                .set_npc_history_directory(
                    bootstrap
                        .config
                        .pack_directory
                        .clone()
                        .ok_or("NPC history requires pack directory")?,
                )?;
            let (pending, archived) = self.npc_inventory.as_ref().expect("loaded NPC inventory");
            self.regions
                .as_mut()
                .expect("started regions")
                .set_npc_recovery_inventory(pending.clone(), archived.clone())?;
            return Ok(None);
        }
        if self.generators.is_none() {
            self.generators = Some(GeneratorService::start(
                bootstrap.store.clone(),
                GeneratorServiceConfig {
                    world_epoch: bootstrap.world_owner.epoch(),
                    capacity: 64,
                    assets: bootstrap.assets.clone(),
                    generation: bootstrap.pack.generation.clone(),
                    treasure_assets: self.assets.treasure.clone(),
                    random: bootstrap.random.root(),
                    drop_plain_wield: self.assets.generators.drop_plain_wield,
                },
            )?);
            return Ok(None);
        }
        if self.saves.is_none() {
            let config = crate::saves::SaveWorkerConfig::default();
            let writer = bootstrap
                .store
                .reserved_writer(config.operation_timeout)
                .await
                .map_err(|e| e.to_string())?;
            self.saves =
                Some(crate::saves::spawn_save_worker(writer, config).map_err(|e| e.to_string())?);
            return Ok(None);
        }
        if self.authentication.is_none() {
            self.authentication = Some(AuthenticationPool::spawn(
                Arc::new(bootstrap.store.clone()),
                &bootstrap.config.network,
                bootstrap.config.accounts.clone(),
            )?);
            return Ok(None);
        }
        if !self.chat_started {
            self.chat =
                crate::chat_service::ChatApiService::start(bootstrap.config.chat_api.clone())
                    .await?;
            self.chat_started = true;
            return Ok(None);
        }
        if self.network.is_none() {
            self.network = Some(
                NetworkThread::spawn(crate::network::NetworkThreadConfig {
                    bind_address: bootstrap
                        .config
                        .bind_address
                        .parse()
                        .map_err(|_| "invalid network bind address")?,
                    portal_time_origin: self.clock.portal_origin,
                    max_sessions: self.limits.sessions,
                    command_capacity: bootstrap.config.command_capacity,
                    event_capacity: self.limits.messages,
                    network: bootstrap.config.network.clone(),
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?,
            );
            return Ok(None);
        }
        if self.simulation.is_none() {
            let kernel = self.kernel.take().ok_or("startup kernel missing")?;
            match SimulationWorker::spawn_with_output_capacity(
                kernel,
                SimulationConfig {
                    command_capacity: bootstrap.config.command_capacity,
                    tick_limit: None,
                    real_time: true,
                },
                self.limits.messages,
            ) {
                Ok(worker) => self.simulation = Some(worker),
                Err(WorkerError::StartupRecovery(owned)) => {
                    self.kernel = Some(owned.kernel);
                    return Err(owned.reason);
                }
                Err(error) => {
                    let message = format!("simulation startup: {error}");
                    self.simulation_failure = Some(error);
                    return Err(message);
                }
            }
        }
        let owners = GameRuntimeOwners {
            bootstrap: self.bootstrap.take().expect("validated startup owner"),
            chat_api: self.chat.take(),
            simulation: self.simulation.take().expect("constructed simulation"),
            network: self.network.take().expect("constructed network"),
            saves: self.saves.take().expect("constructed saves"),
            authentication: self
                .authentication
                .take()
                .expect("constructed authentication"),
            players: self.players.take().expect("constructed lifecycle"),
            online_saves: self.online.take().expect("constructed saves owner"),
            regions: self.regions.take().expect("constructed regions"),
            generators: self.generators.take().expect("constructed generators"),
            preparation: self.preparation.take().expect("constructed preparation"),
        };
        self.finish(owners)
    }
    fn finish(&mut self, owners: GameRuntimeOwners) -> Result<Option<Box<GameRuntime>>, String> {
        match GameRuntime::new(owners, self.assets.runtime.clone(), self.limits, self.clock) {
            Ok(runtime) => {
                self.failure = None;
                Ok(Some(runtime))
            }
            Err((error, owners)) => {
                self.rejected = Some(*owners);
                Err(error)
            }
        }
    }
}
