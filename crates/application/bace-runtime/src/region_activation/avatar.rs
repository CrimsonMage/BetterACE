//! Avatar-only cold DAT closure, using the already fingerprint-admitted archive.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_entry_appearance(
        &mut self,
        sources: &[&bace_content::WeenieV1],
    ) -> Result<crate::player_entry::PreparedEntryAppearanceAssets, String> {
        crate::player_entry::PreparedEntryAppearanceAssets::prepare(&mut self.portal, sources)
    }
    pub fn prepare_avatar_dat(
        &mut self,
        source: &bace_content::WeenieV1,
    ) -> Result<crate::player_assets::PreparedAvatarDat, String> {
        let did = |id| {
            source
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
                .ok_or("missing avatar DAT reference")
        };
        let mut budget = 64 * 1024 * 1024;
        let setup = CollisionSetup::decode(&read(&mut self.portal, did(1)?, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let motions = MotionTable::decode(&read(&mut self.portal, did(2)?, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let maneuvers =
            bace_dat::CombatManeuverTable::decode(&read(&mut self.portal, did(4)?, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let vitals =
            bace_dat::VitalTable::decode(&read(&mut self.portal, 0x0e000003, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let skills = bace_dat::SkillTable::decode(&read(
            &mut self.portal,
            bace_dat::SkillTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let xp = bace_dat::XpTable::decode(&read(
            &mut self.portal,
            bace_dat::XpTable::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let chargen = bace_dat::CharGen::decode(&read(
            &mut self.portal,
            bace_dat::CharGen::RECORD_ID,
            &mut budget,
        )?)
        .map_err(|e| e.to_string())?;
        let spells =
            bace_dat::SpellTable::decode(&read(&mut self.portal, 0x0e00000e, &mut budget)?)
                .map_err(|e| e.to_string())?;
        let quality_filter = Arc::new(
            bace_dat::QualityFilter::decode(&read(
                &mut self.portal,
                bace_dat::QualityFilter::RECORD_ID,
                &mut budget,
            )?)
            .map_err(|e| e.to_string())?,
        );
        let mut animations = BTreeMap::new();
        for data in motions
            .cycles
            .values()
            .chain(motions.modifiers.values())
            .chain(motions.links.values().flat_map(|v| v.values()))
        {
            for segment in &data.animations {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    animations.entry(segment.animation_id)
                {
                    if entry.key() >> 24 != 3 {
                        return Err("invalid avatar animation reference".into());
                    }
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
        let scale = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1.0, |p| p.value) as f32;
        let shape = crate::world_admission::prepare_collision_shape(&setup, scale)?;
        let locomotion = crate::world_admission::prepare_animated_locomotion(
            &motions,
            0x8000003d,
            &animations,
            scale,
        )?;
        let physical_maneuvers =
            crate::world_admission::prepare_combat_maneuvers(&maneuvers, &motions, &animations)?;
        Ok(crate::player_assets::PreparedAvatarDat {
            character: Arc::new(
                crate::character_assets::prepare_character_assets(xp, skills, chargen)
                    .map_err(|e| e.to_string())?,
            ),
            quality_filter,
            vitals,
            spells: Arc::new(spells),
            shape,
            locomotion,
            motions,
            animations,
            maneuvers: physical_maneuvers,
        })
    }
}
