//! GDLE CMotionTable::GetObjectSequence CM_Style branch at the pinned source.
//! Its destination cycle uses the previous style's default substate, deliberately.
use crate::action_chain::{
    ActionChainError, ActionChainRequest, MotionTableSource, ResolvedAction, ResolvedMotionPart,
    resolve_link,
};
pub(crate) fn resolve_style<S: MotionTableSource>(
    source: &S,
    request: ActionChainRequest,
) -> Result<ResolvedAction<'_, S::Data>, ActionChainError> {
    let ActionChainRequest {
        style,
        current,
        current_speed,
        action,
        action_speed,
    } = request;
    if style == 0
        || current == 0
        || action & 0x80000000 == 0
        || !current_speed.is_finite()
        || !action_speed.is_finite()
        || current_speed == 0.0
        || action_speed == 0.0
        || current_speed.abs() > 20.0
        || action_speed.abs() > 20.0
    {
        return Err(ActionChainError::InvalidRequest);
    }
    if style == action {
        let cycle = source
            .cycle(style.wrapping_shl(16) | (current & 0xffffff))
            .ok_or(ActionChainError::MissingCycle)?;
        return Ok(ResolvedAction {
            cycle_rate: crate::MotionRate::Current,
            parts: vec![ResolvedMotionPart {
                rate: crate::MotionRate::Current,
                data: cycle,
                speed: current_speed,
            }],
            cycle,
            completion_clips: 0,
            first_cyclic: 0,
            final_style: style,
            final_substate: current,
            final_speed: current_speed,
            continues_cycle: true,
        });
    }
    let default = source
        .default_motion(style)
        .ok_or(ActionChainError::MissingDefault)?;
    let cycle = source
        .cycle(action.wrapping_shl(16) | (default & 0xffffff))
        .ok_or(ActionChainError::MissingCycle)?;
    let to_default = if current != default {
        resolve_link(source, style, current, current_speed, default, action_speed)
    } else {
        None
    };
    let mut style_link = resolve_link(source, style, default, current_speed, action, action_speed);
    let mut fallback = None;
    if style_link.is_none() {
        style_link = resolve_link(source, style, default, 1.0, source.default_style(), 1.0);
        let middle = source
            .default_motion(source.default_style())
            .ok_or(ActionChainError::MissingDefault)?;
        fallback = resolve_link(source, source.default_style(), middle, 1.0, action, 1.0);
    }
    let mut parts = Vec::with_capacity(4);
    for data in [to_default, style_link, fallback, Some(cycle)]
        .into_iter()
        .flatten()
    {
        parts.push(ResolvedMotionPart {
            rate: crate::MotionRate::Action,
            data,
            speed: action_speed,
        });
    }
    let mut count = 0usize;
    for part in &parts {
        let n = source.animation_count(part.data);
        if n == 0 || n > 255 {
            return Err(ActionChainError::Capacity);
        }
        count = count.checked_add(n).ok_or(ActionChainError::Capacity)?;
    }
    if count > 512 {
        return Err(ActionChainError::Capacity);
    }
    Ok(ResolvedAction {
        cycle_rate: crate::MotionRate::Action,
        parts,
        cycle,
        completion_clips: count - 1,
        first_cyclic: count - 1,
        final_style: action,
        final_substate: default,
        final_speed: action_speed,
        continues_cycle: false,
    })
}
