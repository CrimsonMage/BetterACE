//! Exact authored NPC action/return chains from verified source animation data.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_npc_script_motion(
        &mut self,
        work: &crate::npc_motion_assets::NpcMotionWork,
    ) -> Result<Arc<bace_motion::PreparedMotionChain>, String> {
        let mut budget: usize = 32 * 1024 * 1024;
        if work.table == 0
            || !work.scale.is_finite()
            || work.scale <= 0.0
            || work.scale > 20.0
            || !work.speed.is_finite()
            || work.speed == 0.0
        {
            return Err("NPC motion source bounds".into());
        }
        if !work.target && work.style.unwrap_or(0x8000003d) != work.before.style {
            return Err("NPC authored starting stance is not currently accepted".into());
        }
        if !work.target && work.substyle.is_some_and(|s| s != work.before.substate) {
            return Err("NPC authored starting substate is not currently accepted".into());
        }
        let table = MotionTable::decode(&read(&mut self.portal, work.table, &mut budget)?)
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
        crate::world_admission::prepare_motion_chain(
            &table,
            &animations,
            crate::world_admission::MotionChainRequest {
                style: work.before.style,
                current_motion: work.before.substate,
                current_speed: work.before.speed,
                action: work.motion,
                action_speed: work.speed,
                scale: work.scale,
                modifiers: &[],
            },
        )
    }
}
