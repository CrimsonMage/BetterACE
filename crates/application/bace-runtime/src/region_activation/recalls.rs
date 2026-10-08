//! Cold command-recall action and ACE timer preparation. Marketplace deliberately
//! uses the source's fixed fourteen-second delay, not its longer animation.
use super::*;
use bace_interactions::RecallKind;
pub struct PreparedRecallMotion {
    pub kind: RecallKind,
    pub style: Option<Arc<bace_motion::PreparedMotionChain>>,
    pub chain: Arc<bace_motion::PreparedMotionChain>,
    pub source_animation_seconds: f32,
    pub delay_ticks: u64,
}
pub struct PreparedBindingMotion {
    pub style: Option<Arc<bace_motion::PreparedMotionChain>>,
    pub motion: Arc<bace_motion::PreparedMotionChain>,
    pub seconds: f64,
}
impl VerifiedRegionAssets {
    /// ACE Lifestone/Bindstone.ActOnUse plays Sanctuary after entering
    /// NonCombat. The source timer uses the authored Sanctuary animation.
    pub fn prepare_binding_motion(
        &mut self,
        source: &bace_content::WeenieV1,
        current: bace_motion::SourceMotionState,
    ) -> Result<PreparedBindingMotion, String> {
        const SANCTUARY: u32 = 0x1000_0057;
        let data = self.prepare_avatar_action_data(source)?;
        let style = if current.style != 0x8000_003d {
            Some(data.chain(current, 0x8000_003d)?)
        } else {
            None
        };
        let current = style.as_ref().map_or(current, |chain| {
            chain
                .source_transition()
                .expect("prepared style transition")
                .after
        });
        let source_animation_seconds = source_animation_length(&data.table, SANCTUARY, |id| {
            data.animations.get(&id).map(|a| a.num_frames as usize)
        })?;
        let stance_seconds = style
            .as_ref()
            .map_or(0.0, |chain| chain.nominal_duration_seconds());
        let action_seconds = f64::from(source_animation_seconds);
        let total_seconds = stance_seconds + action_seconds;
        bace_interactions::recall_delay_ticks(RecallKind::Lifestone, total_seconds)
            .map_err(|e| format!("binding timer: {e:?}"))?;
        Ok(PreparedBindingMotion {
            style,
            motion: data.chain(current, SANCTUARY)?,
            seconds: total_seconds,
        })
    }
    /// `current` comes from the accepted motion owner. An authored NonCombat
    /// transition and the recall action are prepared together without mutation.
    pub fn prepare_recall_motion(
        &mut self,
        source: &bace_content::WeenieV1,
        current: bace_motion::SourceMotionState,
        kind: RecallKind,
    ) -> Result<PreparedRecallMotion, String> {
        let data = self.prepare_avatar_action_data(source)?;
        let style = if current.style != 0x8000003d {
            Some(data.chain(current, 0x8000003d)?)
        } else {
            None
        };
        let current = style.as_ref().map_or(current, |chain| {
            chain
                .source_transition()
                .expect("prepared source transition")
                .after
        });
        let source_animation_seconds = source_animation_length(&data.table, kind.motion(), |id| {
            data.animations.get(&id).map(|a| a.num_frames as usize)
        })?;
        let delay_ticks =
            bace_interactions::recall_delay_ticks(kind, f64::from(source_animation_seconds))
                .map_err(|e| format!("recall timer: {e:?}"))?;
        let chain = data.chain(current, kind.motion())?;
        Ok(PreparedRecallMotion {
            kind,
            style,
            chain,
            source_animation_seconds,
            delay_ticks,
        })
    }
}
/// Pinned ACE MotionTable.GetAnimationLength(motion) uses the table's default
/// stance and motion, even if the caller's accepted current substate differs.
/// Preserve f32 accumulation and high-frame clipping. Invalid authored segments
/// are refused before an action can debit resources or enter the world.
pub(super) fn source_animation_length(
    table: &MotionTable,
    motion: u32,
    mut frames: impl FnMut(u32) -> Option<usize>,
) -> Result<f32, String> {
    let stance = table.default_style;
    let current = table.style_defaults.get(&stance).copied().unwrap_or(0);
    let key = (stance << 16) | (current & 0xfffff);
    let Some(link) = table.links.get(&key) else {
        return Ok(0.);
    };
    let Some(data) = link
        .get(&motion)
        .or_else(|| table.links.get(&(stance << 16))?.get(&motion))
    else {
        return Ok(0.);
    };
    let mut seconds = 0.0f32;
    for segment in &data.animations {
        let maximum = frames(segment.animation_id).ok_or("recall animation missing")?;
        let maximum = i32::try_from(maximum).map_err(|_| "recall animation frame capacity")?;
        let high = if segment.high_frame == -1 {
            maximum
        } else {
            segment.high_frame.min(maximum)
        };
        if segment.low_frame < 0
            || high < segment.low_frame
            || !segment.framerate.is_finite()
            || segment.framerate == 0.
        {
            return Err("recall animation frame/rate invalid".into());
        }
        seconds += (high - segment.low_frame) as f32 / segment.framerate.abs();
    }
    if !seconds.is_finite() || seconds > 300. {
        return Err("recall animation duration out of bounds".into());
    }
    Ok(seconds)
}
#[cfg(test)]
mod tests;
