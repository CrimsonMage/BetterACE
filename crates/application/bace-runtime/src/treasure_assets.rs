//! Cold, bounded preparation of pinned ACE treasure dependencies. Call only on
//! asset-preparation capacity, with a fingerprint-admitted portal archive.
use bace_content::WorldRecordV1;
use bace_loot::{TreasureAssets, TreasureColorRow, TreasureMaterialRow, TreasureSpell};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::{collections::BTreeSet, sync::Arc};

/// Explicit template closure keeps preparation from decoding the whole world.
/// Share the returned immutable assets across generators of this generation.
pub fn prepare_treasure_assets(
    generation: &PackGeneration,
    template_ids: &[u32],
    portal: &mut bace_dat::DatArchive,
    version: bace_dat::DatTableVersion,
) -> Result<Arc<TreasureAssets>, String> {
    if template_ids.len() > 16384 {
        return Err("treasure template limit".into());
    }
    let mut assets = TreasureAssets {
        mutation_scripts: Some(Arc::new(
            bace_loot::MutationScripts::pinned()
                .map_err(|e| format!("treasure mutation scripts: {e:?}"))?,
        )),
        ..TreasureAssets::default()
    };
    let mut budget = 128 * 1024 * 1024_usize;
    let mut clothing = BTreeSet::new();
    for id in template_ids.iter().copied().collect::<BTreeSet<_>>() {
        let PackLookup::Record(record) = generation
            .lookup(PackKey {
                namespace: 1,
                id: u64::from(id),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err(format!("missing treasure template {id}"));
        };
        charge(&mut budget, record.bytes().len())?;
        let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
        if template.weenie_id != id {
            return Err("treasure template identity mismatch".into());
        }
        if let Some(value) = template.properties.data_ids.iter().find(|p| p.id == 7) {
            clothing.insert(value.value);
        }
        if template.weenie_type == 34
            && let Some(spell) = template.properties.data_ids.iter().find(|p| p.id == 28)
        {
            // Deterministic duplicate resolution by imported WCID order.
            assets.scrolls_by_spell.entry(spell.value).or_insert(id);
        }
        assets.templates.insert(id, Arc::new(template));
    }
    // Namespaces 40..43 retain imported source IDs as their ordering key.
    let mut cursor = Some(PackKey {
        namespace: 39,
        id: u64::MAX,
    });
    let mut rows = 0usize;
    'scan: loop {
        let batch = generation.scan(cursor, 256).map_err(|e| e.to_string())?;
        if batch.is_empty() {
            break;
        }
        for (key, lookup) in batch {
            cursor = Some(key);
            if key.namespace > 43 {
                break 'scan;
            }
            let PackLookup::Record(record) = lookup else {
                continue;
            };
            rows += 1;
            if rows > 65536 {
                return Err("treasure material row limit".into());
            }
            charge(&mut budget, record.bytes().len())?;
            let row = bace_content_tools::decode_world_record(record.bytes())?;
            if row.namespace() != key.namespace || row.id() != key.id {
                return Err("treasure material identity mismatch".into());
            }
            match row {
                WorldRecordV1::TreasureMaterialBase(r) => assets
                    .material_base
                    .entry((r.material_code, r.tier))
                    .or_default()
                    .push(TreasureMaterialRow {
                        material: r.material_id,
                        probability: r.probability,
                    }),
                WorldRecordV1::TreasureMaterialGroups(r) => assets
                    .material_group
                    .entry((r.material_group, r.tier))
                    .or_default()
                    .push(TreasureMaterialRow {
                        material: r.material_id,
                        probability: r.probability,
                    }),
                WorldRecordV1::TreasureMaterialColor(r) => assets
                    .material_colors
                    .entry((r.material_id, r.color_code))
                    .or_default()
                    .push(TreasureColorRow {
                        palette: r.palette_template,
                        probability: r.probability,
                    }),
                WorldRecordV1::TreasureGemCount(r) => {
                    if r.chance > 0.0 {
                        assets
                            .gem_counts
                            .entry((r.gem_code, r.tier))
                            .or_default()
                            .push((r.count, r.chance));
                    }
                }
                _ => return Err("unexpected treasure dependency row".into()),
            }
        }
    }
    let spells = bace_dat::SpellTable::load_verified(portal, version).map_err(|e| e.to_string())?;
    if spells.spells.len() > 65536 {
        return Err("treasure spell count limit".into());
    }
    for (id, spell) in spells.spells {
        assets.spells.insert(id, prepare_spell(&spell));
    }
    for id in clothing {
        let bytes = portal.read(id).map_err(|e| e.to_string())?;
        charge(&mut budget, bytes.len())?;
        let table = bace_dat::ClothingTable::decode(&bytes).map_err(|e| e.to_string())?;
        if table.id != id {
            return Err("treasure clothing identity mismatch".into());
        }
        assets.clothing_order.insert(id, table.template_order);
        assets
            .clothing_setups
            .insert(id, table.setups.keys().copied().collect());
        assets.clothing_palettes.insert(
            id,
            table
                .templates
                .into_iter()
                .map(|(palette, value)| (palette, value.icon))
                .collect(),
        );
    }
    Ok(Arc::new(assets))
}
fn charge(budget: &mut usize, bytes: usize) -> Result<(), String> {
    *budget = budget
        .checked_sub(bytes)
        .ok_or("treasure preparation byte limit")?;
    Ok(())
}
/// Pinned Spell.Level and SpellFormula.Level use different source rules.
pub fn prepare_spell(spell: &bace_dat::SpellBase) -> TreasureSpell {
    let level = [1, 50, 100, 150, 200, 250, 300, 400]
        .iter()
        .rposition(|power| spell.power >= *power)
        .map_or(0, |index| index as u32 + 1);
    let formula_level = match spell.formula.first().copied() {
        Some(value @ 1..=6) => value,
        Some(110) => 6,
        Some(112 | 192) => 7,
        Some(193) => 8,
        _ => 0,
    };
    TreasureSpell {
        power: spell.power,
        base_mana: spell.base_mana,
        level,
        formula_level,
    }
}
