//! Final equipped NPC animation preparation stays on the existing cold worker.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_npc_loadout_motions(
        &mut self,
        source: &bace_content::WeenieV1,
        loadout: &mut bace_simulation::PreparedNpcLoadout,
        equipment: &[bace_combat::preparation::PhysicalEquipmentSource],
        generation: &bace_storage_codec::PackGeneration,
    ) -> Result<(), String> {
        let id = source
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 2)
            .map(|p| p.value)
            .ok_or("NPC motion table absent")?;
        let scale = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1.0, |p| p.value) as f32;
        let mut budget = 32 * 1024 * 1024;
        let table = MotionTable::decode(&read(&mut self.portal, id, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let mut animations = BTreeMap::new();
        for data in table
            .cycles
            .values()
            .chain(table.modifiers.values())
            .chain(table.links.values().flat_map(|v| v.values()))
        {
            for segment in &data.animations {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    animations.entry(segment.animation_id)
                {
                    entry.insert(
                        Animation::decode(&read(
                            &mut self.portal,
                            segment.animation_id,
                            &mut budget,
                        )?)
                        .map_err(|e| e.to_string())?,
                    );
                }
            }
        }
        let (motions, styles) = crate::physical_assets::prepare_npc_motion_assets(
            &loadout.profile,
            crate::physical_assets::PhysicalMotionAssets {
                table: &table,
                animations: &animations,
                current_motion: 0x41000003,
                current_speed: 1.0,
                scale,
                modifiers: &[],
            },
        )?;
        loadout.death_motions = crate::physical_assets::prepare_death_motion_assets(
            &table,
            &animations,
            &styles,
            scale,
        )?;
        let skills = bace_dat::SkillTable::decode(&read(
            &mut self.portal,
            bace_dat::SkillTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let filter = bace_dat::QualityFilter::decode(&read(
            &mut self.portal,
            bace_dat::QualityFilter::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let mut combat_assets = crate::npc_combat_assets::prepare_npc_combat_assets(
            Arc::new(source.clone()),
            loadout.profile.clone(),
            equipment,
            &skills,
            &filter,
        )?;
        let spells = bace_dat::SpellTable::decode(&read(
            &mut self.portal,
            bace_dat::SpellTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        combat_assets.server_magic = self
            .prepare_proc_magic(
                generation,
                &spells,
                &combat_assets.damage,
                &combat_assets.physical,
            )?
            .batch;
        combat_assets.registry_items = loadout.items.iter().map(|item| item.id).collect();
        combat_assets.registry_items.sort_unstable();
        loadout.combat_assets = Some(Arc::new(combat_assets));
        loadout.physical_motions = motions;
        loadout.locomotion_styles = styles;
        Ok(())
    }
}
