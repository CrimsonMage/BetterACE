//! ACE Healer.DoHealMotion source SkillHealSelf closure, prepared off-thread.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_healing_motion(
        &mut self,
        source: &bace_content::WeenieV1,
        current: bace_motion::SourceMotionState,
    ) -> Result<Arc<bace_motion::PreparedMotionChain>, String> {
        self.prepare_avatar_action_data(source)?
            .chain(current, 0x1000_010e)
    }
}
