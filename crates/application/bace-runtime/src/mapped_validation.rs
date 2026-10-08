//! Checked mixed world-row changes and their small derived index deltas.
use bace_content::{InstanceLinkIndexV1, LandblockIndexV1, WorldRecordV1};
use bace_persistence::MappedContentCandidate;
use bace_storage_codec::{PackGeneration, PackKey, PackLookup, PackRecord};
use std::collections::{BTreeMap, BTreeSet};

type MappedValidation = (Vec<PackRecord>, BTreeSet<u32>, Vec<u32>);

pub(crate) fn validate_mapped(
    generation: &PackGeneration,
    candidates: Vec<MappedContentCandidate>,
) -> Result<MappedValidation, String> {
    let mut output = Vec::with_capacity(candidates.len());
    let mut removed_weenies = BTreeSet::new();
    let mut referenced_weenies = Vec::new();
    let mut landblocks = BTreeMap::<u16, LandblockIndexV1>::new();
    let mut links = BTreeMap::<u32, InstanceLinkIndexV1>::new();
    for candidate in candidates {
        if candidate.schema != 1
            || candidate.id > u64::from(u32::MAX)
            || !matches!(candidate.namespace, 1 | 16..=47 | 50)
            || candidate
                .bytes
                .as_ref()
                .is_some_and(|_| !matches!(candidate.namespace, 16..=45 | 50))
        {
            return Err("unsupported mapped candidate identity or schema".into());
        }
        let key = PackKey {
            namespace: candidate.namespace,
            id: candidate.id,
        };
        let old = match generation.lookup(key).map_err(|e| e.to_string())? {
            PackLookup::Record(record) => Some(record.bytes().to_vec()),
            PackLookup::Missing | PackLookup::Tombstone => None,
        };
        if candidate.bytes.is_none() && old.is_none() {
            return Err("cannot remove a missing immutable record".into());
        }
        if candidate.bytes == old {
            return Err("mapped candidate has no content change".into());
        }
        if candidate.namespace == 1 && candidate.bytes.is_none() {
            removed_weenies.insert(candidate.id as u32);
        }
        if matches!(candidate.namespace, 16..=45) {
            let before = old
                .as_deref()
                .map(bace_content_tools::decode_world_record)
                .transpose()?;
            let after = candidate
                .bytes
                .as_deref()
                .map(bace_content_tools::decode_world_record)
                .transpose()?;
            if before
                .as_ref()
                .is_some_and(|r| r.namespace() != candidate.namespace || r.id() != candidate.id)
                || after
                    .as_ref()
                    .is_some_and(|r| r.namespace() != candidate.namespace || r.id() != candidate.id)
            {
                return Err("world row scalar identity mismatch".into());
            }
            if let Some(before) = &before {
                update_indexes(generation, before, false, &mut landblocks, &mut links)?;
            }
            if let Some(after) = &after {
                match after {
                    WorldRecordV1::LandblockInstance(row) => {
                        referenced_weenies.push(row.weenie_class_id)
                    }
                    WorldRecordV1::Encounter(row) => referenced_weenies.push(row.weenie_class_id),
                    WorldRecordV1::PointsOfInterest(row) => {
                        referenced_weenies.push(row.weenie_class_id)
                    }
                    _ => {}
                }
                update_indexes(generation, after, true, &mut landblocks, &mut links)?;
            }
        } else if candidate.namespace == 50 {
            let Some(bytes) = &candidate.bytes else {
                output.push(PackRecord {
                    key,
                    schema: 1,
                    value: None,
                });
                continue;
            };
            let patch = bace_content_tools::decode_clothing_patch(bytes)?;
            if u64::from(patch.id) != candidate.id {
                return Err("ClothingBase scalar identity mismatch".into());
            }
        }
        output.push(PackRecord {
            key,
            schema: 1,
            value: candidate.bytes,
        });
    }
    for (landblock, mut index) in landblocks {
        index.instance_ids.sort_unstable();
        index.encounter_ids.sort_unstable();
        if index.instance_ids.windows(2).any(|pair| pair[0] == pair[1])
            || index
                .encounter_ids
                .windows(2)
                .any(|pair| pair[0] == pair[1])
            || index.instance_ids.len() + index.encounter_ids.len() > 4096
        {
            return Err("landblock derived index invalid or over capacity".into());
        }
        output.push(PackRecord {
            key: PackKey {
                namespace: 2,
                id: u64::from(landblock),
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
            return Err("instance-link derived index invalid or over capacity".into());
        }
        output.push(PackRecord {
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
    Ok((output, removed_weenies, referenced_weenies))
}

fn update_indexes(
    generation: &PackGeneration,
    row: &WorldRecordV1,
    add: bool,
    landblocks: &mut BTreeMap<u16, LandblockIndexV1>,
    links: &mut BTreeMap<u32, InstanceLinkIndexV1>,
) -> Result<(), String> {
    match row {
        WorldRecordV1::LandblockInstance(r) => {
            let lb = (r.obj_cell_id >> 16) as u16;
            if r.landblock != i32::from(lb) {
                return Err("landblock instance identity mismatch".into());
            }
            let index = landblock_index(generation, landblocks, lb)?;
            edit_id(&mut index.instance_ids, r.guid, add)?;
        }
        WorldRecordV1::Encounter(r) => {
            let lb = u16::try_from(r.landblock).map_err(|_| "encounter landblock range")?;
            let index = landblock_index(generation, landblocks, lb)?;
            edit_id(&mut index.encounter_ids, r.id, add)?;
        }
        WorldRecordV1::LandblockInstanceLink(r) => {
            if let std::collections::btree_map::Entry::Vacant(entry) = links.entry(r.parent_guid) {
                let key = PackKey {
                    namespace: 3,
                    id: u64::from(r.parent_guid),
                };
                let index = match generation.lookup(key).map_err(|e| e.to_string())? {
                    PackLookup::Record(record) => {
                        bace_content_tools::decode_instance_link_index(record.bytes())?
                    }
                    PackLookup::Missing | PackLookup::Tombstone => InstanceLinkIndexV1 {
                        parent_guid: r.parent_guid,
                        children: Vec::new(),
                    },
                };
                if index.parent_guid != r.parent_guid {
                    return Err("instance-link index identity mismatch".into());
                }
                entry.insert(index);
            }
            let entries = &mut links
                .get_mut(&r.parent_guid)
                .expect("loaded index")
                .children;
            if add {
                if entries.iter().any(|entry| entry.link_id == r.id) {
                    return Err("duplicate instance link".into());
                }
                entries.push(bace_content::InstanceLinkTargetV1 {
                    link_id: r.id,
                    child_guid: r.child_guid,
                });
            } else {
                let old_len = entries.len();
                entries.retain(|entry| entry.link_id != r.id);
                if old_len == entries.len() {
                    return Err("missing instance link in derived index".into());
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn landblock_index<'a>(
    generation: &PackGeneration,
    cache: &'a mut BTreeMap<u16, LandblockIndexV1>,
    landblock: u16,
) -> Result<&'a mut LandblockIndexV1, String> {
    if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(landblock) {
        let key = PackKey {
            namespace: 2,
            id: u64::from(landblock),
        };
        let index = match generation.lookup(key).map_err(|e| e.to_string())? {
            PackLookup::Record(record) => {
                bace_content_tools::decode_landblock_index(record.bytes())?
            }
            PackLookup::Missing | PackLookup::Tombstone => LandblockIndexV1 {
                landblock,
                instance_ids: Vec::new(),
                encounter_ids: Vec::new(),
            },
        };
        if index.landblock != landblock {
            return Err("landblock index identity mismatch".into());
        }
        entry.insert(index);
    }
    Ok(cache.get_mut(&landblock).expect("loaded index"))
}

fn edit_id(ids: &mut Vec<u32>, id: u32, add: bool) -> Result<(), String> {
    if add {
        if ids.contains(&id) {
            return Err("duplicate derived world index ID".into());
        }
        ids.push(id);
    } else {
        let old_len = ids.len();
        ids.retain(|entry| *entry != id);
        if old_len == ids.len() {
            return Err("missing derived world index ID".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "mapped_validation/tests.rs"]
mod tests;
