//! Cold join of verified client spell metadata and accepted native spell rows.
//! Archive fingerprint admission belongs to VerifiedRegionAssets; no owner-tick I/O.
use crate::generator_equipment::GeneratorItemSpellAssets;
use bace_content::{SpellRowV1, WeenieV1, WorldRecordV1};
use bace_dat::{DatArchive, SpellTable};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone)]
pub struct PreparedGeneratorSpellAssets {
    pub table: Arc<SpellTable>,
    pub rows: BTreeMap<u32, SpellRowV1>,
}
impl PreparedGeneratorSpellAssets {
    pub fn borrowed(&self) -> GeneratorItemSpellAssets<'_> {
        GeneratorItemSpellAssets {
            table: &self.table,
            rows: &self.rows,
        }
    }
}
pub fn prepare_generator_spell_assets(
    generation: &PackGeneration,
    verified_portal: &mut DatArchive,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
) -> Result<PreparedGeneratorSpellAssets, String> {
    let bytes = verified_portal
        .read(SpellTable::RECORD_ID)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("generator client spell table byte capacity".into());
    }
    let table = Arc::new(SpellTable::decode(&bytes).map_err(|e| e.to_string())?);
    prepare_generator_spell_rows(generation, table, templates)
}
/// Separate decoded-table entry point permits immutable sharing across regions.
pub fn prepare_generator_spell_rows(
    generation: &PackGeneration,
    table: Arc<SpellTable>,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
) -> Result<PreparedGeneratorSpellAssets, String> {
    if templates.len() > 4096 {
        return Err("generator spell template capacity".into());
    }
    let mut required = BTreeSet::new();
    for template in templates.values() {
        if template.properties.spell_book.len() > 256 {
            return Err("generated item spell-book capacity".into());
        }
        for known in &template.properties.spell_book {
            let id = u32::try_from(known.id).map_err(|_| "invalid generated item spell ID")?;
            let spell = table
                .spells
                .get(&id)
                .ok_or_else(|| format!("missing generated item client spell {id}"))?;
            // Source CreateItemSpell has no War/Void branch; no server row is required.
            if matches!(spell.school, 1 | 5) {
                continue;
            }
            required.insert(id);
            if required.len() > 16384 {
                return Err("generator server spell closure capacity".into());
            }
        }
    }
    let mut rows = BTreeMap::new();
    let mut budget = 64 * 1024 * 1024usize;
    for id in required {
        let PackLookup::Record(record) = generation
            .lookup(PackKey {
                namespace: 38,
                id: u64::from(id),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err(format!("missing generated item server spell {id}"));
        };
        budget = budget
            .checked_sub(record.bytes().len())
            .ok_or("generator server spell byte capacity")?;
        let WorldRecordV1::Spell(row) = bace_content_tools::decode_world_record(record.bytes())?
        else {
            return Err("generated item spell namespace mismatch".into());
        };
        if row.id != id {
            return Err("generated item spell identity mismatch".into());
        }
        rows.insert(id, *row);
    }
    Ok(PreparedGeneratorSpellAssets { table, rows })
}
