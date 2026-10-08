//! Player_Death.Die uses MotionTable.GetAnimationLength(Dead), then adds one
//! second in its action chain. The simulation owns that final delay.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_player_death_animation(
        &mut self,
        source: &bace_content::WeenieV1,
    ) -> Result<f64, String> {
        let data = self.prepare_avatar_action_data(source)?;
        let seconds = super::recalls::source_animation_length(&data.table, 0x40000011, |id| {
            data.animations.get(&id).map(|a| a.num_frames as usize)
        })?;
        Ok(f64::from(seconds))
    }
}
