//! Fingerprint-admitted source ClapHands closure, prepared only on cold capacity.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_crafting_motion(
        &mut self,
        source: &bace_content::WeenieV1,
        current: bace_motion::SourceMotionState,
    ) -> Result<Arc<bace_motion::PreparedMotionChain>, String> {
        if current.style != 0x8000003d {
            return Err("crafting requires accepted noncombat motion".into());
        }
        self.prepare_avatar_action_data(source)?
            .chain(current, 0x1300007e)
    }
}
