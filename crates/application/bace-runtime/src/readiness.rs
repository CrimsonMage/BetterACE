//! Startup inventory. Passing data checks never substitutes for gameplay readiness.
use bace_config::ServerConfig;
use bace_db_postgres::PgStore;
use bace_storage_codec::{PackLimits, PackManifest};

pub struct ReadinessReport {
    pub database: bool,
    pub world_records: u64,
    pub active_packs: usize,
    pub dat_files_present: bool,
    pub blockers: Vec<String>,
}

pub async fn inspect(config: &ServerConfig) -> ReadinessReport {
    let mut report = ReadinessReport {
        database: false,
        world_records: 0,
        active_packs: 0,
        dat_files_present: false,
        blockers: Vec::new(),
    };
    match inspect_world(config).await {
        Ok((records, packs)) => {
            report.database = true;
            report.world_records = records;
            report.active_packs = packs;
        }
        Err(error) => report.blockers.push(error),
    }
    report.dat_files_present = config.dat_directory.as_ref().is_some_and(|directory| {
        [
            "client_portal.dat",
            "client_cell_1.dat",
            "client_local_English.dat",
        ]
        .iter()
        .all(|name| directory.join(name).is_file())
    });
    if !report.dat_files_present {
        report
            .blockers
            .push("required local DAT files are absent".into());
    }
    report.blockers.push(
        "stock-client world entry and complete gameplay parity remain unqualified; server-side integration does not establish client playability".into(),
    );
    report
}

async fn inspect_world(config: &ServerConfig) -> Result<(u64, usize), String> {
    let url = config.resolve_database_url().map_err(|e| e.to_string())?;
    let store = PgStore::connect_bounded(&url, 1, std::time::Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;
    let random_path = config.random_key_file.as_deref().ok_or(
        "random_key_file must name a provisioned private RNG key before gameplay admission",
    )?;
    let _random = crate::game_random::load_and_bind(&store, random_path).await?;
    let active = store.active_generation().await.map_err(|e| e.to_string())?;
    store.close().await;
    let active = active.ok_or("no accepted world pack; run bace-cli world-activate")?;
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
    let directory = config
        .pack_directory
        .as_ref()
        .ok_or("pack_directory is not configured")?;
    let worker = crate::pack_io::PackIoWorker::start(
        directory.clone(),
        1,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )?;
    worker
        .try_submit(crate::pack_io::PackJob::Open {
            manifest: manifest.clone(),
        })
        .map_err(|_| "world pack open rejected")?;
    let mut results = worker.shutdown()?;
    results
        .pop()
        .ok_or("missing world pack completion")?
        .result?;
    Ok((manifest.base.record_count, manifest.deltas.len() + 1))
}

impl std::fmt::Display for ReadinessReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "PostgreSQL and accepted world: {}; {} base records in {} active .bace files; DAT files present: {}",
            self.database, self.world_records, self.active_packs, self.dat_files_present
        )?;
        for blocker in &self.blockers {
            writeln!(f, "Not ready: {blocker}")?;
        }
        Ok(())
    }
}
