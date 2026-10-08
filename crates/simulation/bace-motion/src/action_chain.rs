//! Pure GDLE CMotionTable.get_link/GetObjectSequence action-branch selection.
//! Asset adapters implement lookup only; all fallback/order/count rules live here.
pub trait MotionTableSource {
    type Data;
    fn default_motion(&self, style: u32) -> Option<u32>;
    fn link(&self, key: u32, motion: u32) -> Option<&Self::Data>;
    fn cycle(&self, key: u32) -> Option<&Self::Data>;
    fn animation_count(&self, data: &Self::Data) -> usize;
    fn default_style(&self) -> u32;
    fn bitfield(&self, data: &Self::Data) -> u8;
}
#[derive(Clone, Copy, Debug)]
pub struct ActionChainRequest {
    pub style: u32,
    pub current: u32,
    pub current_speed: f32,
    pub action: u32,
    pub action_speed: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionChainError {
    InvalidRequest,
    MissingCycle,
    MissingDefault,
    MissingLink,
    Capacity,
}
pub struct ResolvedMotionPart<'a, D> {
    pub rate: crate::MotionRate,
    pub data: &'a D,
    pub speed: f32,
}
pub struct ResolvedAction<'a, D> {
    pub cycle_rate: crate::MotionRate,
    pub final_style: u32,
    pub parts: Vec<ResolvedMotionPart<'a, D>>,
    pub cycle: &'a D,
    pub completion_clips: usize,
    pub first_cyclic: usize,
    pub final_substate: u32,
    pub final_speed: f32,
    pub continues_cycle: bool,
}
pub fn resolve_link<S: MotionTableSource>(
    source: &S,
    style: u32,
    from: u32,
    from_speed: f32,
    to: u32,
    to_speed: f32,
) -> Option<&S::Data> {
    let reversed = from_speed < 0.0 || to_speed < 0.0;
    let (key, value) = if reversed { (to, from) } else { (from, to) };
    if let Some(link) = source.link(style.wrapping_shl(16) | (key & 0xffffff), value) {
        return Some(link);
    }
    if reversed {
        source.link(
            style.wrapping_shl(16) | (from & 0xffffff),
            source.default_motion(style)?,
        )
    } else {
        source.link(style.wrapping_shl(16), to)
    }
}
pub fn resolve_action<S: MotionTableSource>(
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
        || action & 0x10000000 == 0
        || !current_speed.is_finite()
        || !action_speed.is_finite()
        || current_speed == 0.0
        || action_speed == 0.0
        || current_speed.abs() > 20.0
        || action_speed.abs() > 20.0
    {
        return Err(ActionChainError::InvalidRequest);
    }
    let cycle = source
        .cycle(style.wrapping_shl(16) | (current & 0xffffff))
        .ok_or(ActionChainError::MissingCycle)?;
    let mut parts = Vec::with_capacity(4);
    if let Some(link) = resolve_link(source, style, current, current_speed, action, action_speed) {
        parts.push(ResolvedMotionPart {
            rate: crate::MotionRate::Action,
            data: link,
            speed: action_speed,
        });
    } else {
        let default = source
            .default_motion(style)
            .ok_or(ActionChainError::MissingDefault)?;
        let first = resolve_link(source, style, current, current_speed, default, 1.0)
            .ok_or(ActionChainError::MissingLink)?;
        let action_link = resolve_link(source, style, default, 1.0, action, action_speed)
            .ok_or(ActionChainError::MissingLink)?;
        parts.push(ResolvedMotionPart {
            rate: crate::MotionRate::Unit,
            data: first,
            speed: 1.0,
        });
        parts.push(ResolvedMotionPart {
            rate: crate::MotionRate::Action,
            data: action_link,
            speed: action_speed,
        });
        if let Some(back) = resolve_link(source, style, default, 1.0, current, current_speed) {
            parts.push(ResolvedMotionPart {
                rate: crate::MotionRate::Unit,
                data: back,
                speed: 1.0,
            });
        }
    }
    let mut completion_clips = 0usize;
    for part in &parts {
        let count = source.animation_count(part.data);
        if count == 0 || count > 255 {
            return Err(ActionChainError::Capacity);
        }
        completion_clips = completion_clips
            .checked_add(count)
            .ok_or(ActionChainError::Capacity)?;
    }
    let cycle_count = source.animation_count(cycle);
    if cycle_count == 0 || cycle_count > 255 || completion_clips + cycle_count > 512 {
        return Err(ActionChainError::Capacity);
    }
    parts.push(ResolvedMotionPart {
        rate: crate::MotionRate::Current,
        data: cycle,
        speed: current_speed,
    });
    Ok(ResolvedAction {
        cycle_rate: crate::MotionRate::Current,
        final_style: style,
        parts,
        cycle,
        completion_clips,
        first_cyclic: completion_clips + cycle_count - 1,
        final_substate: current,
        final_speed: current_speed,
        continues_cycle: false,
    })
}

/// Source substate branch for a new server-owned sequence. The existing-cycle
/// speed-only mutation branch needs a live cursor and is explicitly unsupported.
pub fn resolve_sequence<S: MotionTableSource>(
    source: &S,
    request: ActionChainRequest,
) -> Result<ResolvedAction<'_, S::Data>, ActionChainError> {
    if request.action & 0x80000000 != 0 {
        return crate::style_chain::resolve_style(source, request);
    }
    if request.action & 0x10000000 != 0 {
        return resolve_action(source, request);
    }
    let ActionChainRequest {
        style,
        current,
        current_speed,
        action,
        action_speed,
    } = request;
    if style == 0
        || current == 0
        || action & 0x40000000 == 0
        || !current_speed.is_finite()
        || !action_speed.is_finite()
        || current_speed == 0.0
        || action_speed == 0.0
        || current_speed.abs() > 20.0
        || action_speed.abs() > 20.0
    {
        return Err(ActionChainError::InvalidRequest);
    }
    let cycle = source
        .cycle(style.wrapping_shl(16) | (action & 0xffffff))
        .or_else(|| source.cycle(source.default_style().wrapping_shl(16) | (action & 0xffffff)))
        .ok_or(ActionChainError::MissingCycle)?;
    if source.bitfield(cycle) & 2 != 0
        && action != current
        && source.default_motion(style) != Some(current)
    {
        return Err(ActionChainError::InvalidRequest);
    }
    if action == current && current_speed.is_sign_negative() == action_speed.is_sign_negative() {
        return Ok(ResolvedAction {
            cycle_rate: crate::MotionRate::Action,
            final_style: style,
            parts: vec![ResolvedMotionPart {
                rate: crate::MotionRate::Action,
                data: cycle,
                speed: action_speed,
            }],
            cycle,
            completion_clips: 0,
            first_cyclic: 0,
            final_substate: action,
            final_speed: action_speed,
            continues_cycle: true,
        });
    }
    let mut first = resolve_link(source, style, current, current_speed, action, action_speed);
    let mut second = None;
    if first.is_none() || current_speed.is_sign_negative() != action_speed.is_sign_negative() {
        let default = source
            .default_motion(style)
            .ok_or(ActionChainError::MissingDefault)?;
        first = resolve_link(source, style, current, current_speed, default, 1.0);
        second = resolve_link(source, style, default, 1.0, action, action_speed);
    }
    let mut parts = Vec::with_capacity(3);
    if let Some(second) = second {
        if let Some(first) = first {
            parts.push(ResolvedMotionPart {
                rate: crate::MotionRate::Current,
                data: first,
                speed: current_speed,
            });
        }
        parts.push(ResolvedMotionPart {
            rate: crate::MotionRate::Action,
            data: second,
            speed: action_speed,
        });
    } else if let Some(first) = first {
        parts.push(ResolvedMotionPart {
            rate: if current_speed < 0.0 && action_speed > 0.0 {
                crate::MotionRate::NegativeAction
            } else {
                crate::MotionRate::Action
            },
            data: first,
            speed: if current_speed < 0.0 && action_speed > 0.0 {
                -action_speed
            } else {
                action_speed
            },
        });
    }
    parts.push(ResolvedMotionPart {
        rate: crate::MotionRate::Action,
        data: cycle,
        speed: action_speed,
    });
    let mut count = 0usize;
    for part in &parts {
        let value = source.animation_count(part.data);
        if value == 0 || value > 255 {
            return Err(ActionChainError::Capacity);
        }
        count = count.checked_add(value).ok_or(ActionChainError::Capacity)?;
    }
    if count > 512 {
        return Err(ActionChainError::Capacity);
    }
    Ok(ResolvedAction {
        cycle_rate: crate::MotionRate::Action,
        final_style: style,
        parts,
        cycle,
        completion_clips: count - 1,
        first_cyclic: count - 1,
        final_substate: action,
        final_speed: action_speed,
        continues_cycle: false,
    })
}
