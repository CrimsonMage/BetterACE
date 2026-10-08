use bace_db_postgres::PgStore;
use bace_import::{MariaDbBinaries, MariaDbStaging};
use bace_persistence::MappedGeneration;
use bace_storage_codec::{MappedPack, PackLimits};
use std::{path::Path, sync::atomic::AtomicBool};
#[cfg(test)]
mod tests;

pub fn build(
    input: &Path,
    directory: &Path,
    basedir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if directory.exists() {
        return Err("world-build requires a new output directory".into());
    }
    let basedir = basedir
        .ok_or(
            "world-build requires --mariadb-basedir or BACE_MARIADB_BASEDIR for disposable staging",
        )?
        .canonicalize()?;
    let mut backend = MariaDbStaging::new(MariaDbBinaries {
        install_db: basedir.join("bin/mariadb-install-db"),
        server: basedir.join("bin/mariadbd"),
        client: basedir.join("bin/mariadb"),
        basedir,
        library_dir: None,
    });
    let world = bace_import::import_complete_world(&mut backend, input)?;
    let pack = bace_content_tools::build_world_pack(
        &world.weenies,
        &world.records,
        directory,
        &AtomicBool::new(false),
    )?;
    let mut report = toml::Table::new();
    report.insert("source_sha256".into(), world.manifest.source_sha256.into());
    report.insert("upstream_pin".into(), world.manifest.upstream_pin.into());
    report.insert("weenies".into(), (world.weenies.len() as i64).into());
    report.insert("world_rows".into(), (world.records.len() as i64).into());
    let mut counts = toml::Table::new();
    for (table, rows) in world.manifest.table_row_counts {
        counts.insert(table, i64::try_from(rows)?.into());
    }
    report.insert("table_row_counts".into(), counts.into());
    std::fs::write(
        directory.join("import-report.toml"),
        toml::to_string_pretty(&report)?,
    )?;
    println!(
        "Built one aggregate world pack: {} ({} records). Manifest: {}",
        pack.file.display(),
        pack.records,
        pack.manifest.display()
    );
    Ok(())
}

/// Initial world bootstrap. Live replacements must use the publication journal;
/// this command never overwrites an already accepted, different world generation.
pub async fn activate(
    manifest_path: &Path,
    database_url_env: &str,
    reindex: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let limits = PackLimits::default();
    let mut manifest = bace_storage_codec::load_manifest(manifest_path, limits)?;
    if !manifest.deltas.is_empty() {
        return Err("initial world activation requires a single base pack".into());
    }
    let directory = manifest_path.parent().ok_or("manifest directory missing")?;
    // Activation verifies payload integrity and frozen records; subsequent startup
    // only opens the authenticated indexes and lazily loads active regions.
    let pack = MappedPack::open(directory, &manifest.base, limits)?;
    let mut after = None;
    let mut checked = 0_u64;
    let mut logical_records = 0_u64;
    loop {
        let records = pack.scan(after, 1024)?;
        if records.is_empty() {
            break;
        }
        for (key, lookup) in records {
            let bace_storage_codec::PackLookup::Record(record) = lookup else {
                return Err("initial base contains tombstone".into());
            };
            if record.schema() != 1 {
                return Err("unsupported world record schema".into());
            }
            match key.namespace {
                1 => {
                    let value = bace_content_tools::decode(record.bytes())?;
                    if u64::from(value.weenie_id) != key.id {
                        return Err("weenie index identity mismatch".into());
                    }
                }
                2 => {
                    let value = bace_content_tools::decode_landblock_index(record.bytes())?;
                    if u64::from(value.landblock) != key.id {
                        return Err("landblock index identity mismatch".into());
                    }
                }
                3 => {
                    let value = bace_content_tools::decode_instance_link_index(record.bytes())?;
                    if u64::from(value.parent_guid) != key.id {
                        return Err("instance link index identity mismatch".into());
                    }
                }
                48 => {
                    if key.id != 1 {
                        return Err("creature name index identity mismatch".into());
                    }
                    bace_content_tools::decode_creature_names(record.bytes())?;
                }
                49 => {
                    if key.id != 1 {
                        return Err("template class index identity mismatch".into());
                    }
                    bace_content_tools::decode_template_classes(record.bytes())?;
                }
                52 => {
                    let tables = bace_content_tools::decode_treasure_table_set(record.bytes())?;
                    if u64::from(tables.id) != key.id {
                        return Err("treasure table set identity mismatch".into());
                    }
                }
                _ => {
                    let value = bace_content_tools::decode_world_record(record.bytes())?;
                    if value.namespace() != key.namespace || value.id() != key.id {
                        return Err("world index identity mismatch".into());
                    }
                }
            }
            if is_source_record(key.namespace) {
                logical_records += 1;
            }
            after = Some(key);
            checked += 1;
        }
    }
    if checked != manifest.base.record_count {
        return Err("world record count mismatch".into());
    }
    bace_content_tools::validate_world_pack_indexes(&pack)?;
    let url = std::env::var(database_url_env)
        .map_err(|_| format!("{database_url_env} must contain the PostgreSQL URL"))?;
    let store = PgStore::connect(&url, 2).await?;
    let mut parent_hash = None;
    let mut accepted_revision = 0;
    if let Some(active) = store.active_generation().await? {
        if active.manifest_hash == manifest.content_hash(limits)? {
            println!("This world generation is already accepted.");
            store.close().await;
            return Ok(());
        }
        if !reindex {
            return Err(
                "a different generation is already accepted; use journalled content publication"
                    .into(),
            );
        }
        let old = bace_storage_codec::PackManifest::decode(&active.manifest_bytes, limits)?;
        if old.base == manifest.base && old.deltas == manifest.deltas {
            println!("This world pack layout is already accepted.");
            store.close().await;
            return Ok(());
        }
        if !old.deltas.is_empty() {
            return Err("compact the accepted generation before reindexing".into());
        }
        let previous = MappedPack::open(directory, &old.base, limits)?;
        verify_reindex(&previous, &pack, logical_records)?;
        manifest.generation = old
            .generation
            .checked_add(1)
            .ok_or("generation exhausted")?;
        bace_storage_codec::write_manifest(directory, &manifest, limits)?;
        parent_hash = Some(active.manifest_hash);
        accepted_revision = active.accepted_revision;
    }
    let hash = manifest.content_hash(limits)?;
    store
        .accept_mapped(
            None,
            &MappedGeneration {
                manifest_hash: hash,
                parent_hash,
                base_hash: manifest.base.generation,
                accepted_revision,
                manifest_bytes: manifest.encode(limits)?,
            },
        )
        .await?;
    store.close().await;
    println!(
        "Accepted world generation: {checked} checked records in one .bace file. Gameplay readiness is a separate gate."
    );
    Ok(())
}

fn verify_reindex(
    previous: &std::sync::Arc<MappedPack>,
    candidate: &std::sync::Arc<MappedPack>,
    logical_records: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut previous_count = 0_u64;
    let mut after = None;
    loop {
        let rows = previous.scan(after, 1024)?;
        if rows.is_empty() {
            break;
        }
        for (key, old_record) in rows {
            after = Some(key);
            // An existing table set is gameplay content, even though the
            // bootstrap reindex may add namespace 52 to older bases.
            if !is_source_record(key.namespace) && key.namespace != 52 {
                continue;
            }
            let bace_storage_codec::PackLookup::Record(old_record) = old_record else {
                return Err("reindexing requires a compacted base".into());
            };
            let bace_storage_codec::PackLookup::Record(new_record) = candidate.lookup(key)? else {
                return Err("reindexing removed a source record".into());
            };
            if old_record.schema() != new_record.schema()
                || old_record.bytes() != new_record.bytes()
            {
                return Err(
                    "reindexing changed logical content; use the content publication journal"
                        .into(),
                );
            }
            if is_source_record(key.namespace) {
                previous_count += 1;
            }
        }
    }
    if previous_count != logical_records {
        return Err("reindexing added source records".into());
    }
    Ok(())
}

fn is_source_record(namespace: u16) -> bool {
    namespace == 1 || (16..=47).contains(&namespace)
}
