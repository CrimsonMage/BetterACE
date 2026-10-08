//! Exact equipped proc closure. Referenced spell rows/projectiles are prepared on
//! the already verified asset lane before their source profiles become active.
use super::*;
use bace_storage_codec::{PackKey, PackLookup};
impl VerifiedRegionAssets {
    pub fn prepare_proc_magic(
        &mut self,
        generation: &PackGeneration,
        spells: &bace_dat::SpellTable,
        magic: &bace_magic::MagicDamageProfile,
        physical: &bace_gameplay_api::weapon_combat::PhysicalCombatProfile,
    ) -> Result<crate::native_magic_assets::PreparedProcMagic, String> {
        let mut ids = bace_magic::required_proc_spells(magic, physical)
            .map_err(|e| format!("proc dependency: {e:?}"))?;
        ids.extend(bace_combat::physical::physical_dirty_spell_dependencies(
            physical,
        ));
        ids.sort_unstable();
        ids.dedup();
        if ids.len() > 32 {
            return Err("proc dependency capacity".into());
        }
        let required_spells = ids.clone();
        let mut definitions = Vec::with_capacity(ids.len());
        let mut shapes = BTreeMap::new();
        let mut budget = 64 * 1024 * 1024usize;
        for id in ids {
            let PackLookup::Record(record) = generation
                .lookup(PackKey {
                    namespace: 38,
                    id: u64::from(id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err(format!("equipped proc spell {id} unavailable"));
            };
            budget = budget
                .checked_sub(record.bytes().len())
                .ok_or("proc row byte budget")?;
            let bace_content::WorldRecordV1::Spell(row) =
                bace_content_tools::decode_world_record(record.bytes())?
            else {
                return Err("proc spell namespace".into());
            };
            if row.id != id {
                return Err("proc spell index identity".into());
            }
            let base = spells.spells.get(&id).ok_or("proc DAT spell missing")?;
            let projectile = if matches!(base.meta_type, 2 | 10 | 15) {
                let wcid = row
                    .wcid
                    .filter(|id| *id != 0)
                    .ok_or("proc projectile template missing")?;
                let PackLookup::Record(record) = generation
                    .lookup(PackKey {
                        namespace: 1,
                        id: u64::from(wcid),
                    })
                    .map_err(|e| e.to_string())?
                else {
                    return Err("proc projectile native row missing".into());
                };
                budget = budget
                    .checked_sub(record.bytes().len())
                    .ok_or("proc projectile byte budget")?;
                let source =
                    bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
                if source.weenie_id != wcid {
                    return Err("proc projectile index identity".into());
                }
                let shape = if let Some(shape) = shapes.get(&wcid) {
                    Arc::clone(shape)
                } else {
                    let did = source
                        .properties
                        .data_ids
                        .iter()
                        .find(|p| p.id == 1)
                        .map(|p| p.value)
                        .ok_or("proc projectile setup")?;
                    let setup = CollisionSetup::decode(&read(&mut self.portal, did, &mut budget)?)
                        .map_err(|e| e.to_string())?;
                    let scale = source
                        .properties
                        .floats
                        .iter()
                        .find(|p| p.id == 39)
                        .map_or(1., |p| p.value) as f32;
                    let shape = crate::world_admission::prepare_collision_shape(&setup, scale)?;
                    shapes.insert(wcid, shape.clone());
                    shape
                };
                Some((source, shape))
            } else {
                None
            };
            let prepared = crate::native_magic_assets::prepare_instant_native_magic_spell(
                id,
                base,
                &row,
                projectile.as_ref().map(|(w, s)| (w, s.clone())),
            )?;
            definitions.push(prepared.definition);
        }
        Ok(crate::native_magic_assets::PreparedProcMagic {
            required_spells,
            batch: bace_simulation::PreparedMagicAssetBatch {
                definitions,
                projectile_shapes: shapes.into_iter().collect(),
            },
        })
    }
}
