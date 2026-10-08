//! Read-only pack inspection and explicit, immutable offline candidate builds.
use bace_content::{
    CreatureNameIndexV1, CreatureNameV1, InstanceLinkIndexV1, InstanceLinkTargetV1,
    LandblockIndexV1, TemplateClassIdentityV1, TemplateClassIndexV1, WorldRecordV1,
};
use bace_storage_codec::{
    PackGeneration, PackKey, PackLimits, PackLookup, PackManifest, PackRecord, compile_pack,
    load_manifest, write_manifest,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const MAX_FILES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct Change {
    pub key: PackKey,
    pub source: String,
    pub action: &'static str,
}

#[derive(Clone)]
pub(crate) struct Review {
    pub manifest: PackManifest,
    pub manifest_path: PathBuf,
    pub source: PathBuf,
    pub fingerprint: [u8; 32],
    pub changes: Vec<Change>,
    records: Vec<PackRecord>,
}

impl Review {
    pub fn needs_compaction(&self) -> bool {
        self.manifest.deltas.len() == 2
    }

    pub(crate) fn changed_records(&self) -> &[PackRecord] {
        &self.records
    }
}

fn compile_source(kind: &str, source: &str) -> Result<PackRecord, String> {
    let (key, value) = match kind {
        "weenies" => {
            let row = bace_content_tools::parse(source).map_err(|e| e.to_string())?;
            (
                PackKey {
                    namespace: 1,
                    id: u64::from(row.weenie_id),
                },
                bace_content_tools::compile_template(&row).map_err(|e| e.to_string())?,
            )
        }
        "world" => {
            let row = bace_content_tools::parse_world_record(source)?;
            (
                PackKey {
                    namespace: row.namespace(),
                    id: row.id(),
                },
                bace_content_tools::compile_world_record(&row)?,
            )
        }
        "clothing" => {
            let row = bace_content_tools::parse_clothing_patch(source)?;
            (
                PackKey {
                    namespace: 50,
                    id: u64::from(row.id),
                },
                bace_content_tools::compile_clothing_patch(&row)?,
            )
        }
        "loot" => {
            let row = bace_content_tools::parse_loot_graph(source)?;
            (
                PackKey {
                    namespace: 46,
                    id: u64::from(row.id),
                },
                bace_content_tools::compile_loot_graph(&row)?,
            )
        }
        "rares" => {
            let row = bace_content_tools::parse_rare_profile(source)?;
            (
                PackKey {
                    namespace: 47,
                    id: u64::from(row.id),
                },
                bace_content_tools::compile_rare_profile(&row)?,
            )
        }
        "animations" => {
            let row = bace_content_tools::parse_animation_swap(source)?;
            (
                PackKey {
                    namespace: 51,
                    id: u64::from(row.animation_id),
                },
                bace_content_tools::compile_animation_swap(&row)?,
            )
        }
        _ => return Err("Unsupported source folder".into()),
    };
    Ok(PackRecord {
        key,
        schema: 1,
        value: Some(value),
    })
}

pub(crate) fn compile_draft(kind: &str, source: &str) -> Result<PackRecord, String> {
    compile_source(kind, source)
}

pub(crate) fn clone_with_id(source: &str, namespace: u16, id: u32) -> Result<String, String> {
    let mut value: toml::Value = toml::from_str(source).map_err(|e| e.to_string())?;
    let table = value
        .as_table_mut()
        .ok_or("Native content must be a TOML table")?;
    let field = match namespace {
        1 => "weenie_id",
        16..=45 => {
            if namespace == 20 {
                "guid"
            } else {
                "id"
            }
        }
        46 | 47 | 50 => "id",
        51 => "animation_id",
        _ => return Err("Derived indexes cannot be cloned".into()),
    };
    let target = if (16..=45).contains(&namespace) {
        table
            .iter_mut()
            .find_map(|(_, value)| value.as_table_mut())
            .ok_or("World row variant is missing")?
    } else {
        table
    };
    if target
        .insert(field.into(), toml::Value::Integer(i64::from(id)))
        .is_none()
    {
        return Err("Native identity field is missing".into());
    }
    let text = toml::to_string_pretty(&value).map_err(|e| e.to_string())?;
    let folder = source_folder(namespace).ok_or("Unsupported content namespace")?;
    let compiled = compile_source(folder, &text)?;
    if compiled.key
        != (PackKey {
            namespace,
            id: u64::from(id),
        })
    {
        return Err("Cloned identity did not match".into());
    }
    Ok(text)
}

pub(crate) fn editable_record(generation: &PackGeneration, key: PackKey) -> Result<String, String> {
    let PackLookup::Record(handle) = generation.lookup(key).map_err(|e| e.to_string())? else {
        return Err("Record is missing or removed in this generation".into());
    };
    let bytes = handle.bytes();
    match key.namespace {
        1 => bace_content_tools::export_binary(bytes).map_err(|e| e.to_string()),
        16..=45 => bace_content_tools::export_world_record(
            &bace_content_tools::decode_world_record(bytes)?,
        ),
        46 => toml::to_string_pretty(&bace_content_tools::decode_loot_graph(bytes)?)
            .map_err(|e| e.to_string()),
        47 => toml::to_string_pretty(&bace_content_tools::decode_rare_profile(bytes)?)
            .map_err(|e| e.to_string()),
        50 => toml::to_string_pretty(&bace_content_tools::decode_clothing_patch(bytes)?)
            .map_err(|e| e.to_string()),
        51 => toml::to_string_pretty(&bace_content_tools::decode_animation_swap(bytes)?)
            .map_err(|e| e.to_string()),
        _ => Err("Derived index records are read-only; edit their source records".into()),
    }
}

pub(crate) fn source_folder(namespace: u16) -> Option<&'static str> {
    match namespace {
        1 => Some("weenies"),
        16..=45 => Some("world"),
        46 => Some("loot"),
        47 => Some("rares"),
        50 => Some("clothing"),
        51 => Some("animations"),
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Removal {
    kind: String,
    id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Removals {
    remove: Vec<Removal>,
}

fn removal_namespace(kind: &str) -> Option<u16> {
    if kind == "weenie" {
        return Some(1);
    }
    if kind == "loot" {
        return Some(46);
    }
    if kind == "rare" {
        return Some(47);
    }
    if kind == "clothing" {
        return Some(50);
    }
    if kind == "animation" {
        return Some(51);
    }
    // Frozen WorldRecordV1 namespace/table mapping.
    (16..=45).find(|namespace| world_kind(*namespace) == Some(kind))
}

pub(crate) fn world_kind(namespace: u16) -> Option<&'static str> {
    const NAMES: [&str; 30] = [
        "cook_book",
        "encounter",
        "event",
        "house_portal",
        "landblock_instance",
        "landblock_instance_link",
        "points_of_interest",
        "quest",
        "recipe",
        "recipe_mod",
        "recipe_mods_bool",
        "recipe_mods_d_i_d",
        "recipe_mods_float",
        "recipe_mods_i_i_d",
        "recipe_mods_int",
        "recipe_mods_string",
        "recipe_requirements_bool",
        "recipe_requirements_d_i_d",
        "recipe_requirements_float",
        "recipe_requirements_i_i_d",
        "recipe_requirements_int",
        "recipe_requirements_string",
        "spell",
        "treasure_death",
        "treasure_gem_count",
        "treasure_material_base",
        "treasure_material_color",
        "treasure_material_groups",
        "treasure_wielded",
        "version",
    ];
    NAMES.get(usize::from(namespace.checked_sub(16)?)).copied()
}

pub(crate) fn review(manifest_path: &Path, source: &Path) -> Result<Review, String> {
    let limits = PackLimits::default();
    let manifest = load_manifest(manifest_path, limits).map_err(|e| e.to_string())?;
    let pack_dir = manifest_path
        .parent()
        .ok_or("Manifest has no parent folder")?;
    let generation = manifest.open(pack_dir, limits).map_err(|e| e.to_string())?;
    if !source.is_dir()
        || fs::symlink_metadata(source)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("Choose a real native content folder".into());
    }
    let mut inputs = BTreeMap::<PackKey, (PackRecord, String)>::new();
    let mut hasher = Sha256::new();
    hasher.update(manifest.content_hash(limits).map_err(|e| e.to_string())?);
    let mut count = 0usize;
    let mut total = 0usize;
    for folder in [
        "weenies",
        "world",
        "clothing",
        "loot",
        "rares",
        "animations",
        "removals",
    ] {
        let path = source.join(folder);
        if !path.exists() {
            continue;
        }
        if !path.is_dir()
            || fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err(format!("{folder} must be a real directory"));
        }
        let mut files = fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        files.sort();
        for file in files {
            if fs::symlink_metadata(&file)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
                || !file.is_file()
            {
                return Err(format!(
                    "Only regular files are allowed: {}",
                    file.display()
                ));
            }
            if file.extension().and_then(|e| e.to_str()) != Some("toml") {
                return Err(format!("Only .toml files are allowed: {}", file.display()));
            }
            count += 1;
            if count > MAX_FILES {
                return Err("Review exceeds 4096 files".into());
            }
            if fs::metadata(&file).map_err(|e| e.to_string())?.len() > MAX_SOURCE_BYTES as u64 {
                return Err(format!("Source exceeds 16 MiB: {}", file.display()));
            }
            let bytes = fs::read(&file).map_err(|e| e.to_string())?;
            total = total
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_SOURCE_BYTES)
                .ok_or("Review exceeds 16 MiB of source")?;
            let relative = format!(
                "{folder}/{}",
                file.file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("Invalid filename")?
            );
            hasher.update((relative.len() as u64).to_le_bytes());
            hasher.update(relative.as_bytes());
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(&bytes);
            let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            let record = if folder == "removals" {
                let removal: Removal =
                    toml::from_str(text).map_err(|e| format!("{relative}: {e}"))?;
                let namespace = removal_namespace(&removal.kind).ok_or("Unknown removal kind")?;
                PackRecord {
                    key: PackKey {
                        namespace,
                        id: removal.id,
                    },
                    schema: 1,
                    value: None,
                }
            } else {
                compile_source(folder, text).map_err(|e| format!("{relative}: {e}"))?
            };
            if inputs.insert(record.key, (record, relative)).is_some() {
                return Err("Duplicate content identity in source folder".into());
            }
        }
    }
    let removal_manifest = source.join("remove.toml");
    if removal_manifest.exists() {
        if fs::symlink_metadata(&removal_manifest)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
            || fs::metadata(&removal_manifest)
                .map_err(|e| e.to_string())?
                .len()
                > MAX_SOURCE_BYTES as u64
        {
            return Err("Invalid remove.toml".into());
        }
        let bytes = fs::read(&removal_manifest).map_err(|e| e.to_string())?;
        total
            .checked_add(bytes.len())
            .filter(|n| *n <= MAX_SOURCE_BYTES)
            .ok_or("Review exceeds 16 MiB of source")?;
        hasher.update(("remove.toml".len() as u64).to_le_bytes());
        hasher.update(b"remove.toml");
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        let removals: Removals =
            toml::from_str(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        for removal in removals.remove {
            count += 1;
            if count > MAX_FILES {
                return Err("Review exceeds 4096 changes".into());
            }
            let namespace = removal_namespace(&removal.kind).ok_or("Unknown removal kind")?;
            let key = PackKey {
                namespace,
                id: removal.id,
            };
            let record = PackRecord {
                key,
                schema: 1,
                value: None,
            };
            if inputs.insert(key, (record, "remove.toml".into())).is_some() {
                return Err("Duplicate content identity in source folder".into());
            }
        }
    }
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        let name = name.to_str().ok_or("Invalid source filename")?;
        if ![
            "weenies",
            "world",
            "clothing",
            "loot",
            "rares",
            "animations",
            "removals",
            "remove.toml",
        ]
        .contains(&name)
            && ![".DS_Store", "Thumbs.db"].contains(&name)
        {
            return Err(format!("Unexpected source item {name}"));
        }
    }
    let mut records = Vec::new();
    let mut changes = Vec::new();
    for (key, (record, source_name)) in inputs {
        let old = generation.lookup(key).map_err(|e| e.to_string())?;
        let current = match old {
            PackLookup::Record(handle) => Some(handle.bytes().to_vec()),
            PackLookup::Missing | PackLookup::Tombstone => None,
        };
        if record.value == current {
            continue;
        }
        if record.value.is_none() && current.is_none() {
            return Err(format!(
                "Cannot remove missing record {}:{}",
                key.namespace, key.id
            ));
        }
        let action = if record.value.is_none() {
            "Remove"
        } else if current.is_some() {
            "Replace"
        } else {
            "Add"
        };
        changes.push(Change {
            key,
            source: source_name,
            action,
        });
        records.push(record);
    }
    let derived = derive_indexes(&generation, &records)?;
    for record in &derived {
        let old = record_bytes(&generation, record.key)?;
        let action = if record.value.is_none() {
            "Remove"
        } else if old.is_some() {
            "Replace"
        } else {
            "Add"
        };
        changes.push(Change {
            key: record.key,
            source: "generated index".into(),
            action,
        });
    }
    records.extend(derived);
    Ok(Review {
        manifest,
        manifest_path: manifest_path.to_owned(),
        source: source.to_owned(),
        fingerprint: hasher.finalize().into(),
        changes,
        records,
    })
}

fn record_bytes(generation: &PackGeneration, key: PackKey) -> Result<Option<Vec<u8>>, String> {
    match generation.lookup(key).map_err(|e| e.to_string())? {
        PackLookup::Record(record) => Ok(Some(record.bytes().to_vec())),
        PackLookup::Missing | PackLookup::Tombstone => Ok(None),
    }
}

fn update_id(values: &mut Vec<u32>, id: u32, add: bool) -> Result<(), String> {
    if add {
        if values.contains(&id) {
            return Err("Duplicate derived landblock identity".into());
        }
        values.push(id);
    } else {
        let before = values.len();
        values.retain(|value| *value != id);
        if values.len() == before {
            return Err("Missing derived landblock identity".into());
        }
    }
    Ok(())
}

fn update_world_index(
    generation: &PackGeneration,
    row: &WorldRecordV1,
    add: bool,
    landblocks: &mut BTreeMap<u16, LandblockIndexV1>,
    links: &mut BTreeMap<u32, InstanceLinkIndexV1>,
) -> Result<(), String> {
    let landblock = match row {
        WorldRecordV1::LandblockInstance(value) => {
            let id = (value.obj_cell_id >> 16) as u16;
            if value.landblock != i32::from(id) {
                return Err("Landblock instance identity mismatch".into());
            }
            Some((id, value.guid, true))
        }
        WorldRecordV1::Encounter(value) => Some((
            u16::try_from(value.landblock).map_err(|_| "Encounter landblock range")?,
            value.id,
            false,
        )),
        _ => None,
    };
    if let Some((id, member, instance)) = landblock {
        if let std::collections::btree_map::Entry::Vacant(entry) = landblocks.entry(id) {
            let key = PackKey {
                namespace: 2,
                id: u64::from(id),
            };
            let index = record_bytes(generation, key)?
                .map(|bytes| bace_content_tools::decode_landblock_index(&bytes))
                .transpose()?
                .unwrap_or(LandblockIndexV1 {
                    landblock: id,
                    instance_ids: Vec::new(),
                    encounter_ids: Vec::new(),
                });
            if index.landblock != id {
                return Err("Landblock index identity mismatch".into());
            }
            entry.insert(index);
        }
        let index = landblocks.get_mut(&id).expect("loaded landblock index");
        update_id(
            if instance {
                &mut index.instance_ids
            } else {
                &mut index.encounter_ids
            },
            member,
            add,
        )?;
    }
    if let WorldRecordV1::LandblockInstanceLink(value) = row {
        let parent = value.parent_guid;
        if let std::collections::btree_map::Entry::Vacant(entry) = links.entry(parent) {
            let key = PackKey {
                namespace: 3,
                id: u64::from(parent),
            };
            let index = record_bytes(generation, key)?
                .map(|bytes| bace_content_tools::decode_instance_link_index(&bytes))
                .transpose()?
                .unwrap_or(InstanceLinkIndexV1 {
                    parent_guid: parent,
                    children: Vec::new(),
                });
            if index.parent_guid != parent {
                return Err("Instance link index identity mismatch".into());
            }
            entry.insert(index);
        }
        let children = &mut links
            .get_mut(&parent)
            .expect("loaded instance links")
            .children;
        if add {
            if children.iter().any(|entry| entry.link_id == value.id) {
                return Err("Duplicate instance link".into());
            }
            children.push(InstanceLinkTargetV1 {
                link_id: value.id,
                child_guid: value.child_guid,
            });
        } else {
            let before = children.len();
            children.retain(|entry| entry.link_id != value.id);
            if before == children.len() {
                return Err("Missing instance link in index".into());
            }
        }
    }
    Ok(())
}

fn derive_indexes(
    generation: &PackGeneration,
    primary: &[PackRecord],
) -> Result<Vec<PackRecord>, String> {
    let mut landblocks = BTreeMap::<u16, LandblockIndexV1>::new();
    let mut links = BTreeMap::<u32, InstanceLinkIndexV1>::new();
    let mut changed_weenies = BTreeMap::new();
    for record in primary {
        let old = record_bytes(generation, record.key)?;
        if record.key.namespace == 1 {
            if let Some(bytes) = &record.value {
                let weenie = bace_content_tools::decode(bytes).map_err(|e| e.to_string())?;
                if u64::from(weenie.weenie_id) != record.key.id {
                    return Err("Weenie identity mismatch".into());
                }
                changed_weenies.insert(weenie.weenie_id, Some(weenie));
            } else {
                changed_weenies.insert(
                    u32::try_from(record.key.id).map_err(|_| "Weenie ID range")?,
                    None,
                );
            }
        } else if (16..=45).contains(&record.key.namespace) {
            if let Some(old) = old {
                let row = bace_content_tools::decode_world_record(&old)?;
                if row.namespace() != record.key.namespace || row.id() != record.key.id {
                    return Err("Old world row identity mismatch".into());
                }
                update_world_index(generation, &row, false, &mut landblocks, &mut links)?;
            }
            if let Some(bytes) = &record.value {
                let row = bace_content_tools::decode_world_record(bytes)?;
                if row.namespace() != record.key.namespace || row.id() != record.key.id {
                    return Err("New world row identity mismatch".into());
                }
                update_world_index(generation, &row, true, &mut landblocks, &mut links)?;
            }
        }
    }
    let mut derived = Vec::new();
    for (id, mut index) in landblocks {
        index.instance_ids.sort_unstable();
        index.encounter_ids.sort_unstable();
        if index.instance_ids.windows(2).any(|pair| pair[0] == pair[1])
            || index
                .encounter_ids
                .windows(2)
                .any(|pair| pair[0] == pair[1])
            || index.instance_ids.len() + index.encounter_ids.len() > 4096
        {
            return Err("Invalid or oversized derived landblock index".into());
        }
        derived.push(PackRecord {
            key: PackKey {
                namespace: 2,
                id: u64::from(id),
            },
            schema: 1,
            value: if index.instance_ids.is_empty() && index.encounter_ids.is_empty() {
                None
            } else {
                Some(bace_content_tools::compile_landblock_index(&index)?)
            },
        });
    }
    for (parent, mut index) in links {
        index.children.sort_unstable_by_key(|entry| entry.link_id);
        if index
            .children
            .windows(2)
            .any(|pair| pair[0].link_id == pair[1].link_id)
            || index.children.len() > 4096
        {
            return Err("Invalid or oversized instance link index".into());
        }
        derived.push(PackRecord {
            key: PackKey {
                namespace: 3,
                id: u64::from(parent),
            },
            schema: 1,
            value: if index.children.is_empty() {
                None
            } else {
                Some(bace_content_tools::compile_instance_link_index(&index)?)
            },
        });
    }
    if !changed_weenies.is_empty() {
        let class_head = record_bytes(
            generation,
            PackKey {
                namespace: 49,
                id: 1,
            },
        )?;
        let name_head = record_bytes(
            generation,
            PackKey {
                namespace: 48,
                id: 1,
            },
        )?;
        if class_head.is_some() != name_head.is_some() {
            return Err("Pack has only one of the two derived weenie indexes".into());
        }
        if let (Some(class_bytes), Some(name_bytes)) = (class_head, name_head) {
            let mut classes: TemplateClassIndexV1 =
                bace_content_tools::decode_template_classes(&class_bytes)?;
            classes
                .entries
                .retain(|row| !changed_weenies.contains_key(&row.template));
            for (&id, value) in &changed_weenies {
                if let Some(weenie) = value {
                    classes.entries.push(TemplateClassIdentityV1 {
                        template: id,
                        class_name: weenie.class_name.clone(),
                    });
                }
            }
            classes.entries.sort_unstable_by_key(|row| row.template);
            classes.validate()?;
            let compiled = bace_content_tools::compile_template_classes(&classes)?;
            if compiled != class_bytes {
                derived.push(PackRecord {
                    key: PackKey {
                        namespace: 49,
                        id: 1,
                    },
                    schema: 1,
                    value: Some(compiled),
                });
            }

            let mut names: CreatureNameIndexV1 =
                bace_content_tools::decode_creature_names(&name_bytes)?;
            names
                .entries
                .retain(|row| !changed_weenies.contains_key(&row.template));
            for (&id, value) in &changed_weenies {
                if let Some(weenie) = value
                    && weenie.weenie_type == 10
                    && let Some(name) = weenie
                        .properties
                        .strings
                        .iter()
                        .find(|property| property.id == 1)
                {
                    names.entries.push(CreatureNameV1 {
                        template: id,
                        name: name.value.clone(),
                    });
                }
            }
            names.entries.sort_unstable_by_key(|row| row.template);
            names.validate()?;
            let compiled = bace_content_tools::compile_creature_names(&names)?;
            if compiled != name_bytes {
                derived.push(PackRecord {
                    key: PackKey {
                        namespace: 48,
                        id: 1,
                    },
                    schema: 1,
                    value: Some(compiled),
                });
            }
        }
    }
    derived
        .into_iter()
        .filter_map(|record| match record_bytes(generation, record.key) {
            Ok(previous) if previous == record.value => None,
            result => Some(result.map(|_| record)),
        })
        .collect()
}

pub(crate) fn build(
    reviewed: &Review,
    output_parent: &Path,
    approve_compaction: bool,
) -> Result<PathBuf, String> {
    if reviewed.changes.is_empty() {
        return Err("There are no content changes to build".into());
    }
    let current = review(&reviewed.manifest_path, &reviewed.source)?;
    if current.fingerprint != reviewed.fingerprint
        || current
            .changes
            .iter()
            .map(|c| (&c.key, c.action))
            .ne(reviewed.changes.iter().map(|c| (&c.key, c.action)))
    {
        return Err("The pack or source files changed; review again before building".into());
    }
    if current.needs_compaction() && !approve_compaction {
        return Err("Compaction requires separate confirmation".into());
    }
    if !output_parent.is_dir() {
        return Err("Choose an existing output folder".into());
    }
    let output = tempfile::Builder::new()
        .prefix("offline-candidate-")
        .tempdir_in(output_parent)
        .map_err(|e| e.to_string())?;
    let limits = PackLimits::default();
    let source_dir = reviewed
        .manifest_path
        .parent()
        .ok_or("Manifest has no parent folder")?;
    for descriptor in std::iter::once(&current.manifest.base).chain(&current.manifest.deltas) {
        let source = source_dir.join(&descriptor.file_name);
        if fs::symlink_metadata(&source)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Mapped pack path is a symlink".into());
        }
        fs::copy(&source, output.path().join(&descriptor.file_name)).map_err(|e| e.to_string())?;
    }
    let mut manifest = current.manifest.clone();
    if current.needs_compaction() {
        let generation = manifest
            .open(output.path(), limits)
            .map_err(|e| e.to_string())?;
        let base = generation
            .compact(output.path(), limits, &AtomicBool::new(false))
            .map_err(|e| e.to_string())?;
        manifest.base = base;
        manifest.deltas.clear();
    }
    let mut records = current.records;
    records.sort_by_key(|record| record.key);
    if records.windows(2).any(|pair| pair[0].key == pair[1].key) {
        return Err("Duplicate candidate key".into());
    }
    let delta = compile_pack(output.path(), records.into_iter().map(Ok), limits)
        .map_err(|e| e.to_string())?;
    manifest.deltas.push(delta);
    manifest.generation = manifest
        .generation
        .checked_add(1)
        .ok_or("Generation overflow")?;
    let path = write_manifest(output.path(), &manifest, limits).map_err(|e| e.to_string())?;
    manifest
        .open(output.path(), limits)
        .map_err(|e| e.to_string())?;
    let _ = path;
    Ok(output.keep())
}
