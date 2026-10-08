//! Cold shared DAT closure for source avatar actions. No action or pose is accepted here.
use super::*;
pub(super) struct AvatarActionData {
    pub table: MotionTable,
    pub animations: BTreeMap<u32, Animation>,
    pub scale: f32,
}
impl VerifiedRegionAssets {
    pub(super) fn prepare_avatar_action_data(
        &mut self,
        source: &bace_content::WeenieV1,
    ) -> Result<AvatarActionData, String> {
        let id = source
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 2)
            .ok_or("avatar action avatar motion table missing")?
            .value;
        let mut budget = 64 * 1024 * 1024;
        let table = MotionTable::decode(&read(&mut self.portal, id, &mut budget)?)
            .map_err(|e| e.to_string())?;
        let mut animations = BTreeMap::new();
        for data in table
            .cycles
            .values()
            .chain(table.links.values().flat_map(|v| v.values()))
        {
            for segment in &data.animations {
                if animations.len() > 4096 {
                    return Err("avatar action animation closure bounds".into());
                }
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    animations.entry(segment.animation_id)
                {
                    if entry.key() >> 24 != 3 {
                        return Err("avatar action animation closure bounds".into());
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
        Ok(AvatarActionData {
            table,
            animations,
            scale,
        })
    }
}
impl AvatarActionData {
    pub(super) fn chain(
        &self,
        current: bace_motion::SourceMotionState,
        action: u32,
    ) -> Result<Arc<bace_motion::PreparedMotionChain>, String> {
        crate::world_admission::prepare_motion_chain(
            &self.table,
            &self.animations,
            crate::world_admission::MotionChainRequest {
                style: current.style,
                current_motion: current.substate,
                current_speed: current.speed,
                action,
                action_speed: 1.,
                scale: self.scale,
                modifiers: &[],
            },
        )
    }
}
