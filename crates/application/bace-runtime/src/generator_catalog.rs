//! Bounded cold join of generator overload IDs and creature treasure references.
//! Runtime rows retain imported ID order; only required templates are decoded.
use crate::generator_treasure::PreparedGeneratorTreasure;
use bace_content::{TreasureDeathRowV1, TreasureWieldedRowV1, WeenieV1, WorldRecordV1};
use bace_loot::TreasureAssets;
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub struct PreparedGeneratorCatalog {
    pub death: BTreeMap<u32, TreasureDeathRowV1>,
    pub templates: BTreeMap<u32, Arc<WeenieV1>>,
    pub wielded: BTreeMap<u32, Vec<TreasureWieldedRowV1>>,
    pub treasure: BTreeMap<u32, PreparedGeneratorTreasure>,
}
pub fn prepare_generator_catalog(
    generation: &PackGeneration,
    roots: &BTreeMap<u32, Arc<WeenieV1>>,
    assets: Arc<TreasureAssets>,
    aetheria_drop_rate: f32,
) -> Result<PreparedGeneratorCatalog, String> {
    if roots.len() > 4096 {
        return Err("generator catalog root capacity".into());
    }
    if roots.iter().any(|(id, value)| *id != value.weenie_id) {
        return Err("generator root identity mismatch".into());
    }
    let mut budget = 64 * 1024 * 1024usize;
    let mut death_tables = BTreeMap::new();
    let mut death_cursor = Some(PackKey {
        namespace: 38,
        id: u64::MAX,
    });
    let mut death_count = 0;
    'death: loop {
        let batch = generation
            .scan(death_cursor, 256)
            .map_err(|e| e.to_string())?;
        if batch.is_empty() {
            break;
        }
        for (key, value) in batch {
            death_cursor = Some(key);
            if key.namespace != 39 {
                break 'death;
            }
            let PackLookup::Record(record) = value else {
                continue;
            };
            death_count += 1;
            if death_count > 65536 {
                return Err("death treasure row capacity".into());
            }
            charge(&mut budget, record.bytes().len())?;
            let WorldRecordV1::TreasureDeath(row) =
                bace_content_tools::decode_world_record(record.bytes())?
            else {
                return Err("death treasure namespace mismatch".into());
            };
            if u64::from(row.id) != key.id {
                return Err("death treasure row identity mismatch".into());
            }
            // Source FirstOrDefault selects by TreasureType, not the row PK.
            death_tables.entry(row.treasure_type).or_insert(row);
        }
    }
    let mut rows: BTreeMap<u32, Vec<TreasureWieldedRowV1>> = BTreeMap::new();
    let mut cursor = Some(PackKey {
        namespace: 43,
        id: u64::MAX,
    });
    let mut count = 0;
    'scan: loop {
        let batch = generation.scan(cursor, 256).map_err(|e| e.to_string())?;
        if batch.is_empty() {
            break;
        }
        for (key, value) in batch {
            cursor = Some(key);
            if key.namespace != 44 {
                break 'scan;
            }
            let PackLookup::Record(record) = value else {
                continue;
            };
            count += 1;
            if count > 262144 {
                return Err("wielded treasure row capacity".into());
            }
            charge(&mut budget, record.bytes().len())?;
            let WorldRecordV1::TreasureWielded(row) =
                bace_content_tools::decode_world_record(record.bytes())?
            else {
                return Err("wielded treasure namespace mismatch".into());
            };
            if u64::from(row.id) != key.id {
                return Err("wielded treasure identity mismatch".into());
            }
            rows.entry(row.treasure_type).or_default().push(row);
        }
    }
    let mut templates = roots.clone();
    let mut inspected = BTreeSet::new();
    let mut wanted = BTreeSet::new();
    let mut required_wielded = BTreeSet::new();
    let mut overloaded = BTreeSet::new();
    while let Some((&id, template)) = templates.iter().find(|(id, _)| !inspected.contains(*id)) {
        inspected.insert(id);
        wanted.extend(
            template
                .properties
                .create_list
                .iter()
                .map(|r| r.weenie_class_id),
        );
        for row in &template.properties.generators {
            if row.where_create & 0x40 != 0 {
                overloaded.insert(row.weenie_class_id);
            } else {
                wanted.insert(row.weenie_class_id);
            }
        }
        for did in &template.properties.data_ids {
            if did.value == 0 {
                continue;
            }
            match did.id {
                32 => {
                    required_wielded.insert(did.value);
                }
                33 | 35 => {
                    overloaded.insert(did.value);
                }
                _ => {}
            }
        }
        for id in required_wielded.iter().chain(
            overloaded
                .iter()
                .filter(|id| !death_tables.contains_key(id)),
        ) {
            if let Some(table) = rows.get(id) {
                wanted.extend(table.iter().map(|r| r.weenie_class_id));
            }
        }
        wanted.remove(&0);
        wanted.remove(&3666);
        for id in std::mem::take(&mut wanted) {
            if templates.contains_key(&id) {
                continue;
            }
            if templates.len() >= 4096 {
                return Err("generator template closure capacity".into());
            }
            let value = if let Some(value) = assets.templates.get(&id) {
                value.clone()
            } else {
                let PackLookup::Record(record) = generation
                    .lookup(PackKey {
                        namespace: 1,
                        id: u64::from(id),
                    })
                    .map_err(|e| e.to_string())?
                else {
                    return Err(format!("missing generator dependency template {id}"));
                };
                charge(&mut budget, record.bytes().len())?;
                let value =
                    bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
                if value.weenie_id != id {
                    return Err("generator dependency identity mismatch".into());
                }
                Arc::new(value)
            };
            if value.weenie_id != id {
                return Err("generator dependency identity mismatch".into());
            }
            templates.insert(id, value);
        }
    }
    let mut prepared_assets = (*assets).clone();
    prepared_assets
        .templates
        .extend(templates.iter().map(|(&id, value)| (id, value.clone())));
    let prepared_assets = Arc::new(prepared_assets);
    let mut treasure = BTreeMap::new();
    death_tables.retain(|id, _| overloaded.contains(id));
    for id in overloaded {
        let death = death_tables.get(&id).cloned();
        let value = PreparedGeneratorTreasure::prepare(
            id,
            death,
            rows.get(&id).cloned().unwrap_or_default(),
            prepared_assets.clone(),
            aetheria_drop_rate,
        )
        .map_err(|e| format!("generator treasure {id}: {e:?}"))?;
        treasure.insert(id, value);
    }
    let mut wielded = BTreeMap::new();
    for id in required_wielded {
        let table = rows
            .remove(&id)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("missing creature wielded treasure {id}"))?;
        wielded.insert(id, table);
    }
    Ok(PreparedGeneratorCatalog {
        death: death_tables,
        templates,
        wielded,
        treasure,
    })
}
fn charge(budget: &mut usize, bytes: usize) -> Result<(), String> {
    *budget = budget
        .checked_sub(bytes)
        .ok_or("generator catalog byte capacity")?;
    Ok(())
}
