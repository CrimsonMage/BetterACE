//! Tooling acceptance of derived indexes against the immutable source rows.
//! No whole decoded world or member set is retained; this is not a startup scan.
use bace_content::WorldRecordV1;
use bace_storage_codec::{MappedPack, PackKey, PackLimits, PackLookup, RecordHandle};
use std::sync::Arc;

#[derive(Default, PartialEq, Eq, Debug)]
struct Counts {
    instances: u64,
    encounters: u64,
    links: u64,
}
fn add(count: &mut u64, amount: usize, maximum: u64) -> Result<(), String> {
    *count = count
        .checked_add(amount as u64)
        .filter(|n| *n <= maximum)
        .ok_or("derived index member count exceeds pack record bound")?;
    Ok(())
}
fn schema(record: &RecordHandle) -> Result<(), String> {
    if record.schema() != 1 {
        return Err("unsupported source/index record schema".into());
    }
    Ok(())
}
fn source(pack: &Arc<MappedPack>, namespace: u16, id: u32) -> Result<WorldRecordV1, String> {
    let key = PackKey {
        namespace,
        id: u64::from(id),
    };
    let PackLookup::Record(record) = pack.lookup(key).map_err(|e| e.to_string())? else {
        return Err(format!(
            "derived index references missing source {namespace}/{id}"
        ));
    };
    schema(&record)?;
    let row = crate::decode_world_record(record.bytes())?;
    if row.namespace() != namespace || row.id() != key.id {
        return Err(format!(
            "derived index source identity differs from key {namespace}/{id}"
        ));
    }
    Ok(row)
}
fn strict(values: &[u32]) -> Result<(), String> {
    if values.windows(2).any(|p| p[0] >= p[1]) {
        return Err("derived index members must be strictly ordered by source ID".into());
    }
    Ok(())
}

/// Prove indexes equal the canonical source representation before publication.
///
/// Every member must resolve to exactly the keyed source row and its one owner.
/// Index keys are unique in the pack and member lists are strictly increasing,
/// so a source row cannot be counted twice (even in another owner's index).
/// Equality of indexed/source counts then proves completeness without hashes or
/// a world-sized decoded member set. Empty, missing, extra, misplaced, reordered
/// and partial indexes fail. Dangling authored child GUIDs remain valid source
/// data: links are compared to source link rows, not to absent DAT/static objects.
///
/// Memory is one bounded decoded index plus one source row and mapped handles.
/// Total membership lookups are bounded by the pack record count. This performs
/// I/O/checksums and belongs only in tooling/acceptance, never lazy startup/ticks.
pub fn validate_world_pack_indexes(pack: &Arc<MappedPack>) -> Result<(), String> {
    crate::creature_name_validation::validate(pack)?;
    let maximum = pack.descriptor().record_count;
    if maximum > PackLimits::default().max_records {
        return Err("world pack record count exceeds validation bound".into());
    }
    let mut actual = Counts::default();
    let mut indexed = Counts::default();
    let mut cursor = None;
    while let Some((key, value)) = pack.scan(cursor, 1).map_err(|e| e.to_string())?.pop() {
        cursor = Some(key);
        if !matches!(key.namespace, 2 | 3 | 17 | 20 | 21) {
            continue;
        }
        let PackLookup::Record(record) = value else {
            return Err("complete world source/index contains a tombstone".into());
        };
        schema(&record)?;
        match key.namespace {
            2 => {
                let index = crate::decode_landblock_index(record.bytes())?;
                if key.id != u64::from(index.landblock) {
                    return Err("landblock index identity differs from key".into());
                }
                if index.instance_ids.is_empty() && index.encounter_ids.is_empty() {
                    return Err("empty landblock index has no canonical source members".into());
                }
                strict(&index.instance_ids)?;
                strict(&index.encounter_ids)?;
                add(&mut indexed.instances, index.instance_ids.len(), maximum)?;
                add(&mut indexed.encounters, index.encounter_ids.len(), maximum)?;
                for id in index.instance_ids {
                    let WorldRecordV1::LandblockInstance(row) = source(pack, 20, id)? else {
                        return Err("wrong instance source variant".into());
                    };
                    if row.landblock != i32::from(index.landblock)
                        || row.obj_cell_id >> 16 != u32::from(index.landblock)
                    {
                        return Err(format!("instance {id} belongs to another landblock"));
                    }
                }
                for id in index.encounter_ids {
                    let WorldRecordV1::Encounter(row) = source(pack, 17, id)? else {
                        return Err("wrong encounter source variant".into());
                    };
                    if row.landblock != i32::from(index.landblock) {
                        return Err(format!("encounter {id} belongs to another landblock"));
                    }
                }
            }
            3 => {
                let index = crate::decode_instance_link_index(record.bytes())?;
                if key.id != u64::from(index.parent_guid) {
                    return Err("parent link index identity differs from key".into());
                }
                if index.children.is_empty() {
                    return Err("empty parent link index has no canonical source members".into());
                }
                if index
                    .children
                    .windows(2)
                    .any(|p| p[0].link_id >= p[1].link_id)
                {
                    return Err(
                        "parent link members must be strictly ordered by source link ID".into(),
                    );
                }
                add(&mut indexed.links, index.children.len(), maximum)?;
                for member in index.children {
                    let WorldRecordV1::LandblockInstanceLink(row) =
                        source(pack, 21, member.link_id)?
                    else {
                        return Err("wrong instance link source variant".into());
                    };
                    if row.parent_guid != index.parent_guid || row.child_guid != member.child_guid {
                        return Err(format!(
                            "link {} parent/child differs from source",
                            member.link_id
                        ));
                    }
                }
            }
            17 => add(&mut actual.encounters, 1, maximum)?,
            20 => add(&mut actual.instances, 1, maximum)?,
            21 => add(&mut actual.links, 1, maximum)?,
            _ => unreachable!("filtered index/source namespace"),
        }
    }
    if actual != indexed {
        return Err(format!(
            "incomplete world indexes: source={actual:?}, indexed={indexed:?}"
        ));
    }
    Ok(())
}
