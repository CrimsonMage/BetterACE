//! Explicit operator review of bounded native TOML dropped into local folders.
//! Scanning is request driven; no watcher or startup import touches these files.
use bace_admin::{
    ContentAction, ContentChange, ContentPreview, ContentReply, ContentRequest, ContentRevision,
    ContentStatus,
};
use bace_config::ServerConfig;
use bace_db_postgres::PgStore;
use bace_persistence::{ContentCandidate, MappedContentCandidate, NativeContentCandidate};
use bace_storage_codec::{PackGeneration, PackKey, PackLimits, PackLookup, PackManifest};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::Path,
    sync::Arc,
};
use tokio::sync::mpsc;

const MAX_FILES: usize = 4096;
const MAX_BYTES: usize = 16 * 1024 * 1024;

struct Snapshot {
    accepted_hash: [u8; 32],
    preview: ContentPreview,
    weenies: Vec<ContentCandidate>,
    native: Vec<NativeContentCandidate>,
    mapped: Vec<MappedContentCandidate>,
}

/// At most eight outstanding review tokens are retained. Publish re-reads the
/// inbox and accepted generation before committing the exact reviewed batch.
pub(crate) async fn run(config: ServerConfig, mut requests: mpsc::Receiver<ContentRequest>) {
    let mut previews = VecDeque::<String>::new();
    let mut store: Option<PgStore> = None;
    while let Some(request) = requests.recv().await {
        if store.is_none() {
            match connect(&config).await {
                Ok(connected) => store = Some(connected),
                Err(error) => {
                    let _ = request.reply.send(Err(error));
                    continue;
                }
            }
        }
        let store = store.as_ref().expect("connected store");
        let result = match request.action {
            ContentAction::Status => status(store).await.map(ContentReply::Status),
            ContentAction::Revision { revision } => revision_status(store, revision)
                .await
                .map(ContentReply::Revision),
            ContentAction::StageRemoval { kind, id } => stage_removal(&config, store, kind, id)
                .await
                .map(|path| ContentReply::Staged { path }),
            ContentAction::Preview => match scan_async(&config, store).await {
                Ok(next) => {
                    let preview = next.preview.clone();
                    if preview.total != 0 {
                        previews.retain(|token| token != &preview.token);
                        if previews.len() == 8 {
                            previews.pop_front();
                        }
                        previews.push_back(preview.token.clone());
                    }
                    Ok(ContentReply::Preview(preview))
                }
                Err(error) => Err(error),
            },
            ContentAction::Publish { token } => {
                let Some(position) = previews.iter().position(|reviewed| reviewed == &token) else {
                    let _ = request
                        .reply
                        .send(Err("Review the inbox again before publishing.".into()));
                    continue;
                };
                previews.remove(position);
                match scan_async(&config, store).await {
                    Ok(current) if current.preview.token == token && current.preview.total != 0 => {
                        queue(store, current).await
                    }
                    Ok(_) => Err("Inbox files or accepted pack changed; review again.".into()),
                    Err(error) => Err(error),
                }
            }
        };
        let _ = request.reply.send(result);
    }
}

async fn status(store: &PgStore) -> Result<ContentStatus, String> {
    let journal = store.content_status().await.map_err(|e| e.to_string())?;
    let active = store.active_generation().await.map_err(|e| e.to_string())?;
    let pack_generation = active
        .map(|generation| {
            PackManifest::decode(&generation.manifest_bytes, PackLimits::default())
                .map(|manifest| manifest.generation)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    Ok(ContentStatus {
        accepted_revision: journal.accepted_revision,
        pending_publications: journal.pending_publications,
        rejected_publications: journal.rejected_publications,
        pack_generation,
    })
}

async fn revision_status(store: &PgStore, revision: i64) -> Result<ContentRevision, String> {
    let decision = store
        .publication_status(revision)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Content revision was not found")?;
    Ok(ContentRevision {
        revision,
        status: decision.status,
        rejection: decision.rejection,
    })
}

async fn queue(store: &PgStore, snapshot: Snapshot) -> Result<ContentReply, String> {
    let revision = store
        .insert_mapped_batch(
            &snapshot.weenies,
            &snapshot.native,
            &snapshot.mapped,
            snapshot.accepted_hash,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(ContentReply::Queued { revision })
}

async fn stage_removal(
    config: &ServerConfig,
    store: &PgStore,
    kind: String,
    id: u64,
) -> Result<String, String> {
    let namespace = removal_namespace(&kind).ok_or("Unknown content kind for removal")?;
    if id > u64::from(u32::MAX) {
        return Err("Content ID exceeds supported range".into());
    }
    let accepted = store
        .active_generation()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No accepted .bace generation")?;
    let directory = config
        .pack_directory
        .clone()
        .ok_or("pack_directory is required")?;
    let inbox = config.content_inbox_directory.clone();
    tokio::task::spawn_blocking(move || {
        let manifest = PackManifest::decode(&accepted.manifest_bytes, PackLimits::default())
            .map_err(|e| e.to_string())?;
        if manifest
            .content_hash(PackLimits::default())
            .map_err(|e| e.to_string())?
            != accepted.manifest_hash
        {
            return Err("Accepted manifest hash mismatch".into());
        }
        let generation = manifest
            .open(&directory, PackLimits::default())
            .map_err(|e| e.to_string())?;
        let key = PackKey { namespace, id };
        if !matches!(
            generation.lookup(key).map_err(|e| e.to_string())?,
            PackLookup::Record(_)
        ) {
            return Err(format!("{kind} {id} is not present in the accepted pack"));
        }
        write_removal_file(&inbox, &kind, id)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn write_removal_file(inbox: &Path, kind: &str, id: u64) -> Result<String, String> {
    fs::create_dir_all(inbox).map_err(|e| e.to_string())?;
    if !fs::symlink_metadata(inbox)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_dir()
    {
        return Err("Content inbox must be a real directory".into());
    }
    let folder = inbox.join("removals");
    match fs::symlink_metadata(&folder) {
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err("removals must be a real directory".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&folder).map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    }
    let mut total = 0;
    for entry in fs::read_dir(&folder).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let bytes = read_file(&path, &mut total)?;
        let existing: Removal =
            toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if existing.kind == kind && existing.id == id {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Invalid removal filename")?;
            return Ok(format!("removals/{name}"));
        }
    }
    let nonce = crate::control::random_bytes().map_err(|e| e.to_string())?;
    let name: String = nonce[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let file_name = format!("{name}.toml");
    let path = folder.join(&file_name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    write!(file, "kind = {kind:?}\nid = {id}\n").map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(format!("removals/{file_name}"))
}

async fn scan_async(config: &ServerConfig, store: &PgStore) -> Result<Snapshot, String> {
    let active = store
        .active_generation()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No accepted .bace generation; activate a reviewed base before using the inbox.")?;
    let directory = config
        .pack_directory
        .clone()
        .ok_or("pack_directory is required for inbox review")?;
    let inbox = config.content_inbox_directory.clone();
    tokio::task::spawn_blocking(move || {
        let manifest = PackManifest::decode(&active.manifest_bytes, PackLimits::default())
            .map_err(|e| e.to_string())?;
        if manifest
            .content_hash(PackLimits::default())
            .map_err(|e| e.to_string())?
            != active.manifest_hash
        {
            return Err("Accepted manifest hash mismatch".into());
        }
        let generation = Arc::new(
            manifest
                .open(&directory, PackLimits::default())
                .map_err(|e| e.to_string())?,
        );
        scan(&inbox, active.manifest_hash, &generation)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn connect(config: &ServerConfig) -> Result<PgStore, String> {
    let url = std::env::var(&config.database_url_env).map_err(|_| {
        format!(
            "Set {} to the PostgreSQL connection URL.",
            config.database_url_env
        )
    })?;
    let store = PgStore::connect_bounded(&url, 1, std::time::Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;
    store.migrate().await.map_err(|e| e.to_string())?;
    Ok(store)
}

fn scan(
    root: &Path,
    accepted_hash: [u8; 32],
    generation: &PackGeneration,
) -> Result<Snapshot, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    if fs::symlink_metadata(root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Content inbox directory must not be a symlink.".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(accepted_hash);
    let mut entries = Vec::new();
    let mut weenies = Vec::new();
    let mut native = Vec::new();
    let mut mapped = Vec::new();
    let mut seen = BTreeSet::new();
    let mut source_bytes = 0usize;
    for (folder, kind) in [
        ("weenies", "weenie"),
        ("world", "world"),
        ("clothing", "clothing"),
        ("loot", "loot"),
        ("rares", "rare"),
    ] {
        let path = root.join(folder);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(format!("{folder} inbox path must be a real directory"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|e| e.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
        let mut files = fs::read_dir(&path)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        files.sort();
        for file in files {
            if file
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_os_metadata)
            {
                continue;
            }
            if file.extension().and_then(|e| e.to_str()) != Some("toml") {
                return Err(format!("Only .toml source files are allowed in {folder}/"));
            }
            let relative = format!(
                "{folder}/{}",
                file.file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("Invalid inbox filename")?
            );
            let bytes = read_file(&file, &mut source_bytes)?;
            fingerprint(&mut hasher, &relative, &bytes);
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("{relative}: {e}"))?;
            let (key, compiled, label, name) =
                compile(kind, text).map_err(|e| format!("{relative}: {e}"))?;
            insert_change(
                generation,
                &mut seen,
                &mut entries,
                &mut weenies,
                &mut native,
                &mut mapped,
                key,
                Some(compiled),
                relative,
                label,
                name,
            )?;
        }
    }
    let removal_folder = root.join("removals");
    match fs::symlink_metadata(&removal_folder) {
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err("removals inbox path must be a real directory".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&removal_folder).map_err(|e| e.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    }
    let mut removal_files = fs::read_dir(&removal_folder)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|value| value.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    removal_files.sort();
    for file in removal_files {
        if file
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_os_metadata)
        {
            continue;
        }
        if file.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            return Err("Only .toml removal files are allowed in removals/".into());
        }
        let relative = format!(
            "removals/{}",
            file.file_name()
                .and_then(|name| name.to_str())
                .ok_or("Invalid removal filename")?
        );
        let bytes = read_file(&file, &mut source_bytes)?;
        fingerprint(&mut hasher, &relative, &bytes);
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let removal: Removal = toml::from_str(text).map_err(|e| format!("{relative}: {e}"))?;
        let namespace = removal_namespace(&removal.kind).ok_or("Unknown removal kind")?;
        insert_change(
            generation,
            &mut seen,
            &mut entries,
            &mut weenies,
            &mut native,
            &mut mapped,
            PackKey {
                namespace,
                id: removal.id,
            },
            None,
            relative,
            removal.kind,
            String::new(),
        )?;
    }
    let removals = root.join("remove.toml");
    let has_removals = match fs::symlink_metadata(&removals) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.to_string()),
    };
    if has_removals {
        let bytes = read_file(&removals, &mut source_bytes)?;
        fingerprint(&mut hasher, "remove.toml", &bytes);
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let manifest: RemovalManifest = toml::from_str(text).map_err(|e| e.to_string())?;
        for removal in manifest.remove {
            let namespace = removal_namespace(&removal.kind)
                .ok_or_else(|| format!("Unknown remove kind {}", removal.kind))?;
            insert_change(
                generation,
                &mut seen,
                &mut entries,
                &mut weenies,
                &mut native,
                &mut mapped,
                PackKey {
                    namespace,
                    id: removal.id,
                },
                None,
                "remove.toml".into(),
                removal.kind,
                String::new(),
            )?;
        }
    }
    for root_entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let name = root_entry.map_err(|e| e.to_string())?.file_name();
        let name = name.to_str().ok_or("Invalid inbox root filename")?;
        if is_os_metadata(name) {
            continue;
        }
        if !matches!(
            name,
            "weenies" | "world" | "clothing" | "loot" | "rares" | "removals" | "remove.toml"
        ) {
            return Err(format!(
                "Unexpected inbox item {name}; use a named folder or remove.toml"
            ));
        }
    }
    let total = entries.len();
    let compiled_bytes = weenies
        .iter()
        .map(|c| c.bytes.len())
        .chain(native.iter().map(|c| c.bytes.len()))
        .chain(mapped.iter().map(|c| c.bytes.as_ref().map_or(0, Vec::len)))
        .try_fold(0usize, |sum, n| sum.checked_add(n));
    if total > MAX_FILES || compiled_bytes.is_none_or(|n| n > MAX_BYTES) {
        return Err("Inbox exceeds 4096 changes or 16 MiB of compiled content.".into());
    }
    if total != 0 {
        crate::native_publication::validate_candidates(
            generation,
            weenies.clone(),
            native.clone(),
            mapped.clone(),
        )
        .map_err(|e| format!("Inbox batch validation failed: {e}"))?;
    }
    let source_hash: [u8; 32] = hasher.finalize().into();
    let token = source_hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let preview = ContentPreview {
        token,
        entries,
        total,
    };
    Ok(Snapshot {
        accepted_hash,
        preview,
        weenies,
        native,
        mapped,
    })
}

fn is_os_metadata(name: &str) -> bool {
    matches!(name, ".DS_Store" | "Thumbs.db")
}

fn read_file(path: &Path, total: &mut usize) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err(format!(
            "{} must be a regular file no larger than 16 MiB",
            path.display()
        ));
    }
    *total = total
        .checked_add(metadata.len() as usize)
        .ok_or("Inbox size overflow")?;
    if *total > MAX_BYTES {
        return Err("Inbox source files exceed 16 MiB.".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() as u64 != metadata.len() {
        return Err("Inbox file changed while reading; review again.".into());
    }
    Ok(bytes)
}

fn fingerprint(hasher: &mut Sha256, relative: &str, bytes: &[u8]) {
    hasher.update((relative.len() as u64).to_le_bytes());
    hasher.update(relative.as_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn compile(kind: &str, text: &str) -> Result<(PackKey, Vec<u8>, String, String), String> {
    match kind {
        "weenie" => {
            let template = bace_content_tools::parse(text).map_err(|e| e.to_string())?;
            let id = template.weenie_id;
            let name = template.class_name.clone();
            Ok((
                PackKey {
                    namespace: 1,
                    id: u64::from(id),
                },
                bace_content_tools::compile_template(&template).map_err(|e| e.to_string())?,
                "weenie".into(),
                name,
            ))
        }
        "world" => {
            let row = bace_content_tools::parse_world_record(text)?;
            let label = row.table_name().to_string();
            let name = match &row {
                bace_content::WorldRecordV1::Quest(value) => value.name.clone(),
                bace_content::WorldRecordV1::Event(value) => value.name.clone(),
                bace_content::WorldRecordV1::PointsOfInterest(value) => value.name.clone(),
                _ => label.clone(),
            };
            let key = PackKey {
                namespace: row.namespace(),
                id: row.id(),
            };
            Ok((
                key,
                bace_content_tools::compile_world_record(&row)?,
                label,
                name,
            ))
        }
        "clothing" => {
            let patch = bace_content_tools::parse_clothing_patch(text)?;
            Ok((
                PackKey {
                    namespace: 50,
                    id: u64::from(patch.id),
                },
                bace_content_tools::compile_clothing_patch(&patch)?,
                "clothing".into(),
                format!("0x{:08X}", patch.id),
            ))
        }
        "loot" => {
            let graph = bace_content_tools::parse_loot_graph(text)?;
            Ok((
                PackKey {
                    namespace: 46,
                    id: u64::from(graph.id),
                },
                bace_content_tools::compile_loot_graph(&graph)?,
                "loot".into(),
                format!("profile {}", graph.id),
            ))
        }
        "rare" => {
            let profile = bace_content_tools::parse_rare_profile(text)?;
            Ok((
                PackKey {
                    namespace: 47,
                    id: u64::from(profile.id),
                },
                bace_content_tools::compile_rare_profile(&profile)?,
                "rare".into(),
                format!("profile {}", profile.id),
            ))
        }
        _ => Err("Unsupported inbox folder".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_change(
    generation: &PackGeneration,
    seen: &mut BTreeSet<PackKey>,
    entries: &mut Vec<ContentChange>,
    weenies: &mut Vec<ContentCandidate>,
    native: &mut Vec<NativeContentCandidate>,
    mapped: &mut Vec<MappedContentCandidate>,
    key: PackKey,
    bytes: Option<Vec<u8>>,
    path: String,
    kind: String,
    name: String,
) -> Result<(), String> {
    if key.id > u64::from(u32::MAX) {
        return Err(format!(
            "{} ID exceeds supported mapped content range",
            key.id
        ));
    }
    if !seen.insert(key) {
        return Err(format!(
            "Duplicate content key {}:{} in inbox",
            key.namespace, key.id
        ));
    }
    if seen.len() > MAX_FILES {
        return Err("Inbox change count exceeds 4096".into());
    }
    let lookup = generation.lookup(key).map_err(|e| e.to_string())?;
    // A retained removal file is harmless after its reviewed tombstone lands.
    // An ID that was never in the pack still fails review below.
    if bytes.is_none() && matches!(&lookup, PackLookup::Tombstone) {
        return Ok(());
    }
    let old = match &lookup {
        PackLookup::Record(record) => Some(record.bytes()),
        PackLookup::Missing | PackLookup::Tombstone => None,
    };
    if bytes.is_some() && bytes.as_deref() == old {
        return Ok(());
    }
    let action = match (&bytes, old) {
        (None, None) => return Err(format!("{kind} {} does not exist in accepted pack", key.id)),
        (None, Some(_)) => "remove",
        (Some(_), None) => "add",
        (Some(_), Some(_)) => "replace",
    };
    match (key.namespace, bytes) {
        (1, Some(bytes)) => {
            let template = bace_content_tools::decode(&bytes).map_err(|e| e.to_string())?;
            weenies.push(ContentCandidate {
                wcid: template.weenie_id,
                class_name: template.class_name,
                weenie_type: i32::try_from(template.weenie_type)
                    .map_err(|_| "weenie type range")?,
                bytes,
            });
        }
        (46 | 47, Some(bytes)) => native.push(NativeContentCandidate {
            namespace: key.namespace,
            id: key.id as u32,
            schema: 1,
            bytes,
        }),
        (_, bytes) => mapped.push(MappedContentCandidate {
            namespace: key.namespace,
            id: key.id,
            schema: 1,
            bytes,
        }),
    }
    let name = if name.is_empty() {
        existing_name(key, old).unwrap_or_else(|| kind.clone())
    } else {
        name
    };
    let name: String = name
        .chars()
        .take(120)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    entries.push(ContentChange {
        path,
        kind,
        id: key.id,
        name,
        action: action.into(),
    });
    Ok(())
}

fn existing_name(key: PackKey, old: Option<&[u8]>) -> Option<String> {
    let bytes = old.filter(|bytes| bytes.len() <= 1024 * 1024)?;
    match key.namespace {
        1 => bace_content_tools::decode(bytes)
            .ok()
            .map(|weenie| weenie.class_name),
        18 | 22 | 23 => match bace_content_tools::decode_world_record(bytes).ok()? {
            bace_content::WorldRecordV1::Event(value) => Some(value.name),
            bace_content::WorldRecordV1::PointsOfInterest(value) => Some(value.name),
            bace_content::WorldRecordV1::Quest(value) => Some(value.name),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemovalManifest {
    remove: Vec<Removal>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Removal {
    kind: String,
    id: u64,
}

fn removal_namespace(kind: &str) -> Option<u16> {
    Some(match kind {
        "weenie" => 1,
        "cook_book" => 16,
        "encounter" => 17,
        "event" => 18,
        "house_portal" => 19,
        "landblock_instance" => 20,
        "landblock_instance_link" => 21,
        "points_of_interest" => 22,
        "quest" => 23,
        "recipe" => 24,
        "recipe_mod" => 25,
        "recipe_mods_bool" => 26,
        "recipe_mods_d_i_d" => 27,
        "recipe_mods_float" => 28,
        "recipe_mods_i_i_d" => 29,
        "recipe_mods_int" => 30,
        "recipe_mods_string" => 31,
        "recipe_requirements_bool" => 32,
        "recipe_requirements_d_i_d" => 33,
        "recipe_requirements_float" => 34,
        "recipe_requirements_i_i_d" => 35,
        "recipe_requirements_int" => 36,
        "recipe_requirements_string" => 37,
        "spell" => 38,
        "treasure_death" => 39,
        "treasure_gem_count" => 40,
        "treasure_material_base" => 41,
        "treasure_material_color" => 42,
        "treasure_material_groups" => 43,
        "treasure_wielded" => 44,
        "version" => 45,
        "loot" => 46,
        "rare" => 47,
        "clothing" => 50,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "content_inbox/tests.rs"]
mod tests;
