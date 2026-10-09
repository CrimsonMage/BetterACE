//! Production startup resources. All blocking pack and DAT work stays outside
//! simulation; the exclusive database world owner survives every live service.
use crate::{
    game_random::BoundGameplayRandom,
    pack_io::{PackIoWorker, PackJob, PreparedPack},
    region_activation::RegionAssetManifest,
    world_settings::PreparedWorldSettings,
};
use bace_config::ServerConfig;
use bace_db_postgres::{PgStore, WorldOwner};
use bace_persistence::StoredAllegiance;
use bace_storage_codec::{PackLimits, PackManifest};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Timestamp pair captured by the adapter, never by a gameplay/physics owner.
#[derive(Clone, Copy, Debug)]
pub struct GameClock {
    pub unix_millis: u64,
    pub monotonic: Instant,
    pub portal_origin: f64,
}
impl GameClock {
    pub fn capture() -> Result<Self, String> {
        let monotonic = Instant::now();
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock before Unix epoch")?;
        Self::from_pair(
            u64::try_from(elapsed.as_millis()).map_err(|_| "system clock overflow")?,
            monotonic,
        )
    }
    pub fn from_pair(unix_millis: u64, monotonic: Instant) -> Result<Self, String> {
        // ACE.Common.DerethDateTime.UtcNowToEMUTime subtracts Jan 31, 2017
        // noon EST (17:00 UTC), with the fixed -5h source offset (not DST).
        const EMU_ORIGIN_MILLIS: u64 = 1_485_882_000_000;
        let elapsed = unix_millis
            .checked_sub(EMU_ORIGIN_MILLIS)
            .ok_or("system clock precedes ACE EMU origin")?;
        let portal_origin = elapsed as f64 / 1000.;
        crate::network::PortalClock::new(portal_origin, 0).map_err(str::to_owned)?;
        Ok(Self {
            unix_millis,
            monotonic,
            portal_origin,
        })
    }
    pub fn unix_seconds(&self) -> u64 {
        self.unix_millis / 1000
    }
}

pub struct GameBootstrap {
    pub config: ServerConfig,
    pub store: PgStore,
    pub world_owner: WorldOwner,
    pub random: BoundGameplayRandom,
    pub world_settings: PreparedWorldSettings,
    pub assets: RegionAssetManifest,
    pub pack: PreparedPack,
    /// Temporarily transferred to the retained publication future while it runs.
    pub pack_io: Option<PackIoWorker>,
    pub save_pressure: Arc<AtomicBool>,
    pub allegiance_nodes: Vec<StoredAllegiance>,
    pub allegiance_metadata: Vec<StoredAllegiance>,
}
impl GameBootstrap {
    /// Complete validation before opening game sockets. This reopens the accepted
    /// immutable pack generation; it never recompiles/decodes the whole world.
    pub async fn prepare(config: ServerConfig) -> Result<Self, String> {
        config.validate().map_err(|e| e.to_string())?;
        let world_settings = crate::world_settings::prepare_world_settings(&config.world)?;
        let directory = config
            .dat_directory
            .clone()
            .ok_or("dat_directory is required for gameplay")?;
        let assets = approved_manifest(&directory);
        let manifest = assets.clone();
        tokio::task::spawn_blocking(move || verify_assets(&manifest, &directory))
            .await
            .map_err(|e| format!("asset validation worker: {e}"))??;
        let url = config.resolve_database_url().map_err(|e| e.to_string())?;
        let store =
            PgStore::connect_bounded(&url, config.database_connections, Duration::from_secs(30))
                .await
                .map_err(|e| e.to_string())?;
        store.migrate().await.map_err(|e| e.to_string())?;
        let random = crate::game_random::load_and_bind(
            &store,
            config
                .random_key_file
                .as_deref()
                .ok_or("random_key_file is required for gameplay")?,
        )
        .await?;
        let active = store
            .active_generation()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("no accepted world pack")?;
        let manifest = PackManifest::decode(&active.manifest_bytes, PackLimits::default())
            .map_err(|e| e.to_string())?;
        if manifest
            .content_hash(PackLimits::default())
            .map_err(|e| e.to_string())?
            != active.manifest_hash
            || manifest.base.generation != active.base_hash
        {
            return Err("accepted world manifest metadata mismatch".into());
        }
        let save_pressure = Arc::new(AtomicBool::new(false));
        let pack_io = PackIoWorker::start(
            config
                .pack_directory
                .clone()
                .ok_or("pack_directory is required for gameplay")?,
            2,
            save_pressure.clone(),
        )?;
        let job = pack_io
            .try_submit_tracked(PackJob::Open { manifest })
            .map_err(|_| "accepted pack open queue rejected")?;
        let pack = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                match pack_io.try_recv() {
                    Ok(completed) => {
                        if completed.job_id != job {
                            return Err("unexpected pack startup completion".into());
                        }
                        return completed.result;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err("pack worker closed during startup".into());
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        tokio::time::sleep(Duration::from_millis(1)).await
                    }
                }
            }
        })
        .await
        .map_err(|_| "accepted pack open deadline exceeded")??;
        let mut world_owner = store
            .acquire_world_owner()
            .await
            .map_err(|e| e.to_string())?;
        world_owner
            .recover_characters()
            .await
            .map_err(|e| e.to_string())?;
        let (allegiance_nodes, allegiance_metadata) = store
            .load_allegiance_forest(65536, 64 * 1024 * 1024)
            .await
            .map_err(|e| e.to_string())?;
        // Validate the complete indexed forest before any game admission; retain
        // frozen rows/version fences for subsequent atomic social operations.
        crate::social_saves::restore_allegiances(&allegiance_nodes, &allegiance_metadata, 65536)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            config,
            store,
            world_owner,
            random,
            world_settings,
            assets,
            pack,
            pack_io: Some(pack_io),
            save_pressure,
            allegiance_nodes,
            allegiance_metadata,
        })
    }
    pub fn configure_kernel(
        &self,
        kernel: &mut bace_simulation::Kernel,
        clock: GameClock,
        levels: Arc<bace_character::CharacterLevelTable>,
    ) -> Result<(), String> {
        kernel
            .require_prepared_generator_births()
            .map_err(|e| format!("cold generator birth policy: {e:?}"))?;
        kernel
            .configure_chat_policy(crate::chat_policy::prepare_chat_policy(
                &self.config.chat_policy,
            ))
            .map_err(|e| format!("chat startup policy: {e:?}"))?;
        self.random
            .configure_kernel(kernel, self.world_owner.epoch(), clock.unix_seconds())?;
        kernel
            .configure_generators(
                self.random.root(),
                i64::try_from(clock.unix_seconds()).map_err(|_| "generator clock overflow")?,
                crate::game_clock::generator_is_day(clock.portal_origin)?,
            )
            .map_err(|e| format!("generator startup: {e:?}"))?;
        kernel
            .set_social_epoch(
                i64::try_from(clock.unix_seconds()).map_err(|_| "social clock overflow")?,
            )
            .map_err(|e| format!("social clock: {e:?}"))?;
        kernel
            .configure_social_experience(levels)
            .map_err(|e| format!("experience tables: {e:?}"))?;
        let forest = crate::social_saves::restore_allegiances(
            &self.allegiance_nodes,
            &self.allegiance_metadata,
            65536,
        )
        .map_err(|e| e.to_string())?;
        kernel
            // GDLE checkpoint intervals consume simulation elapsed time. Unix
            // timestamps belong to social presence metadata, not this clock.
            .register_allegiances(forest, kernel.ticks() as f64 / 30.0)
            .map_err(|e| format!("allegiance startup: {e:?}"))?;
        self.world_settings.clone().install(kernel)
    }
}
fn approved_manifest(directory: &std::path::Path) -> RegionAssetManifest {
    // docs/baselines.toml [assets], supplied DAT set; proprietary files stay local.
    RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    }
}
fn verify_assets(
    manifest: &RegionAssetManifest,
    directory: &std::path::Path,
) -> Result<(), String> {
    for (path, expected) in [
        (&manifest.portal, manifest.portal_sha256.as_str()),
        (&manifest.cell, manifest.cell_sha256.as_str()),
        (
            &directory.join("client_local_English.dat"),
            "e85c820280c88fac7df6c8043f5e24596e9c8774193af4123d756546f78fb2bb",
        ),
    ] {
        if bace_dat::fingerprint(path).map_err(|e| e.to_string())? != expected {
            return Err(format!("unapproved DAT fingerprint: {}", path.display()));
        }
    }
    Ok(())
}
