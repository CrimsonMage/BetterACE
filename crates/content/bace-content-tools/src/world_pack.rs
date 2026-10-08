//! One immutable aggregate file for all world records, with lazy per-record reads.
use crate::PackBuild;
use bace_content::{
    InstanceLinkIndexV1, InstanceLinkTargetV1, LandblockIndexV1, WeenieTemplate, WorldRecordV1,
};
use bace_storage_codec::{CodecLimits, PackError, PackKey, PackLimits, PackManifest, PackRecord};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

/// Kind 2/schema 1 stores frozen typed world rows, never SQL or JSON.
pub fn compile_world_record(record: &WorldRecordV1) -> Result<Vec<u8>, String> {
    record.validate()?;
    bace_storage_codec::encode(2, 1, record, CodecLimits::default()).map_err(|e| e.to_string())
}
pub fn decode_world_record(bytes: &[u8]) -> Result<WorldRecordV1, String> {
    let record: WorldRecordV1 = bace_storage_codec::decode(bytes, 2, 1, CodecLimits::default())
        .map_err(|e| e.to_string())?;
    record.validate()?;
    Ok(record)
}
pub fn decode_landblock_index(bytes: &[u8]) -> Result<LandblockIndexV1, String> {
    bace_storage_codec::decode(bytes, 3, 1, CodecLimits::default()).map_err(|e| e.to_string())
}
pub fn compile_landblock_index(index: &LandblockIndexV1) -> Result<Vec<u8>, String> {
    if index.instance_ids.len() > 100_000 || index.encounter_ids.len() > 100_000 {
        return Err("landblock index limit".into());
    }
    bace_storage_codec::encode(3, 1, index, CodecLimits::default()).map_err(|e| e.to_string())
}

pub fn decode_instance_link_index(bytes: &[u8]) -> Result<InstanceLinkIndexV1, String> {
    bace_storage_codec::decode(bytes, 4, 1, CodecLimits::default()).map_err(|e| e.to_string())
}
pub fn compile_instance_link_index(index: &InstanceLinkIndexV1) -> Result<Vec<u8>, String> {
    if index.children.len() > 100_000 {
        return Err("instance link index limit".into());
    }
    bace_storage_codec::encode(4, 1, index, CodecLimits::default()).map_err(|e| e.to_string())
}

/// Offline compiler. The input is a complete validated import or native data set.
/// Emits one aggregate `.bace` base plus its manifest; never one file per entity.
/// Additional memory is a compact borrowed key index and per-landblock ID lists;
/// only one bounded encoded record is retained by the streaming pack compiler.
pub fn build_world_pack(
    weenies: &[WeenieTemplate],
    records: &[WorldRecordV1],
    directory: &Path,
    cancel: &AtomicBool,
) -> Result<PackBuild, String> {
    let limits = PackLimits::default();
    if weenies.is_empty() || weenies.len() > 100_000 || records.len() as u64 > limits.max_records {
        return Err("world source count limit".into());
    }
    let mut landblocks = BTreeMap::<u16, LandblockIndexV1>::new();
    let mut creature_names = bace_content::CreatureNameIndexV1 {
        schema_version: 1,
        entries: Vec::new(),
    };
    let mut name_bytes = 0usize;
    for weenie in weenies.iter().filter(|w| w.weenie_type == 10) {
        if let Some(name) = weenie.properties.strings.iter().find(|p| p.id == 1) {
            name_bytes = name_bytes
                .checked_add(name.value.len())
                .filter(|n| *n <= 8 * 1024 * 1024)
                .ok_or("creature name index byte budget")?;
            creature_names.entries.push(bace_content::CreatureNameV1 {
                template: weenie.weenie_id,
                name: name.value.clone(),
            });
        }
    }
    creature_names.entries.sort_unstable_by_key(|r| r.template);
    creature_names.validate()?;
    if weenies
        .iter()
        .try_fold(0usize, |n, w| n.checked_add(w.class_name.len()))
        .is_none_or(|n| n > 8 * 1024 * 1024)
    {
        return Err("class-name index byte budget".into());
    }
    let mut classes = bace_content::TemplateClassIndexV1 {
        schema_version: 1,
        entries: weenies
            .iter()
            .map(|w| bace_content::TemplateClassIdentityV1 {
                template: w.weenie_id,
                class_name: w.class_name.clone(),
            })
            .collect(),
    };
    classes.entries.sort_unstable_by_key(|r| r.template);
    classes.validate()?;
    let mut links = BTreeMap::<u32, InstanceLinkIndexV1>::new();
    for record in records {
        if let WorldRecordV1::LandblockInstanceLink(r) = record {
            let index = links
                .entry(r.parent_guid)
                .or_insert_with(|| InstanceLinkIndexV1 {
                    parent_guid: r.parent_guid,
                    children: Vec::new(),
                });
            if index.children.len() >= 100_000 {
                return Err("instance link index limit".into());
            }
            index.children.push(InstanceLinkTargetV1 {
                link_id: r.id,
                child_guid: r.child_guid,
            });
        }
        let (lb, instance, encounter) = match record {
            WorldRecordV1::LandblockInstance(row) => {
                let lb = (row.obj_cell_id >> 16) as u16;
                if row.landblock != i32::from(lb) {
                    return Err("landblock instance generated identity mismatch".into());
                }
                (lb, Some(row.guid), None)
            }
            WorldRecordV1::Encounter(row) => (
                u16::try_from(row.landblock).map_err(|_| "encounter landblock range")?,
                None,
                Some(row.id),
            ),
            _ => continue,
        };
        let index = landblocks.entry(lb).or_insert_with(|| LandblockIndexV1 {
            landblock: lb,
            instance_ids: Vec::new(),
            encounter_ids: Vec::new(),
        });
        if let Some(id) = instance {
            index.instance_ids.push(id);
        }
        if let Some(id) = encounter {
            index.encounter_ids.push(id);
        }
        if index.instance_ids.len() > 100_000 || index.encounter_ids.len() > 100_000 {
            return Err("landblock index limit".into());
        }
    }
    enum Input<'a> {
        CreatureNames(&'a bace_content::CreatureNameIndexV1),
        Classes(&'a bace_content::TemplateClassIndexV1),
        Weenie(&'a WeenieTemplate),
        Index(&'a LandblockIndexV1),
        Links(&'a InstanceLinkIndexV1),
        World(&'a WorldRecordV1),
        TreasureTables(&'a bace_content::TreasureTableSetV1),
    }
    let treasure_tables = crate::parse_treasure_table_set(include_str!(
        "../../../gameplay/bace-loot/data/ace-treasure-tables.toml"
    ))?;
    let mut sorted = Vec::with_capacity(weenies.len() + records.len() + landblocks.len());
    sorted.push((
        PackKey {
            namespace: 48,
            id: 1,
        },
        Input::CreatureNames(&creature_names),
    ));
    sorted.push((
        PackKey {
            namespace: 49,
            id: 1,
        },
        Input::Classes(&classes),
    ));
    sorted.push((
        PackKey {
            namespace: 52,
            id: u64::from(treasure_tables.id),
        },
        Input::TreasureTables(&treasure_tables),
    ));
    for w in weenies {
        sorted.push((
            PackKey {
                namespace: 1,
                id: u64::from(w.weenie_id),
            },
            Input::Weenie(w),
        ));
    }
    for (lb, index) in &mut landblocks {
        index.instance_ids.sort_unstable();
        index.encounter_ids.sort_unstable();
        sorted.push((
            PackKey {
                namespace: 2,
                id: u64::from(*lb),
            },
            Input::Index(index),
        ));
    }
    for (parent, index) in &mut links {
        index.children.sort_unstable_by_key(|c| c.link_id);
        sorted.push((
            PackKey {
                namespace: 3,
                id: u64::from(*parent),
            },
            Input::Links(index),
        ));
    }
    for r in records {
        sorted.push((
            PackKey {
                namespace: r.namespace(),
                id: r.id(),
            },
            Input::World(r),
        ));
    }
    sorted.sort_unstable_by_key(|r| r.0);
    if sorted.windows(2).any(|p| p[0].0 == p[1].0) {
        return Err("duplicate world record identity".into());
    }
    let records = sorted.into_iter().map(|(key, input)| {
        if cancel.load(Ordering::Relaxed) {
            return Err(PackError::Format("world build cancelled"));
        }
        let bytes = match input {
            Input::CreatureNames(index) => crate::compile_creature_names(index)
                .map_err(|_| PackError::Format("invalid creature name index"))?,
            Input::Classes(index) => crate::compile_template_classes(index)
                .map_err(|_| PackError::Format("invalid template class index"))?,
            Input::Weenie(w) => {
                crate::compile_template(w).map_err(|_| PackError::Format("invalid weenie"))?
            }
            Input::Index(i) => bace_storage_codec::encode(3, 1, i, CodecLimits::default())
                .map_err(|_| PackError::Format("invalid landblock index"))?,
            Input::Links(i) => bace_storage_codec::encode(4, 1, i, CodecLimits::default())
                .map_err(|_| PackError::Format("invalid instance link index"))?,
            Input::World(w) => {
                compile_world_record(w).map_err(|_| PackError::Format("invalid world row"))?
            }
            Input::TreasureTables(t) => crate::compile_treasure_table_set(t)
                .map_err(|_| PackError::Format("invalid treasure tables"))?,
        };
        Ok(PackRecord {
            key,
            schema: 1,
            value: Some(bytes),
        })
    });
    let descriptor =
        bace_storage_codec::compile_pack(directory, records, limits).map_err(|e| e.to_string())?;
    let manifest = PackManifest {
        version: 1,
        generation: 1,
        base: descriptor.clone(),
        deltas: Vec::new(),
    };
    let manifest = bace_storage_codec::write_manifest(directory, &manifest, limits)
        .map_err(|e| e.to_string())?;
    Ok(PackBuild {
        file: directory.join(descriptor.file_name),
        manifest,
        records: descriptor.record_count,
    })
}

/// Checked native TOML boundary for a single frozen world row.
pub fn parse_world_record(source: &str) -> Result<WorldRecordV1, String> {
    if source.len() > 16 * 1024 * 1024 {
        return Err("world TOML exceeds 16 MiB".into());
    }
    let record: WorldRecordV1 = toml::from_str(source).map_err(|e| e.to_string())?;
    record.validate()?;
    Ok(record)
}
pub fn export_world_record(record: &WorldRecordV1) -> Result<String, String> {
    record.validate()?;
    toml::to_string_pretty(record).map_err(|e| e.to_string())
}
