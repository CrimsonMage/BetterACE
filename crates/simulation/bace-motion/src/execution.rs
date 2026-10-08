//! GDLE CSequence double-cursor execution at 353cbab52ef7da2b7063bc3e3f008461d8531693.
//! Inclusive clip bounds and source hook crossing/completion semantics. Immutable
//! programs are prepared off-thread; execution is bounded and performs no I/O.
use crate::{MotionHook, MotionHookPayload, MotionPhysics, RootFrame, TimelineError};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MotionDomain {
    Casting,
    Physical,
    Interaction,
    Inventory,
    Crafting,
    Recall,
    Death,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MotionToken {
    pub domain: MotionDomain,
    pub owner: u64,
    pub sequence: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionClip {
    pub animation: u32,
    pub frame_count: u32,
    pub low: i32,
    pub high: i32,
    pub framerate: f32,
    pub frames: Arc<[RootFrame]>,
    pub hooks: Arc<[MotionHook]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMotionChain {
    pub(super) rate_model: Option<Arc<crate::MotionRateModel>>,
    clips: Vec<ExecutionClip>,
    first_cyclic: usize,
    completion_clips: usize,
    pub physics: MotionPhysics,
    pub motion: u32,
    pub speed: f32,
    source: Option<SourceMotionTransition>,
    stop: Option<Arc<PreparedMotionChain>>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceMotionState {
    pub style: u32,
    pub substate: u32,
    pub speed: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceMotionTransition {
    pub before: SourceMotionState,
    pub after: SourceMotionState,
    pub continues_cycle: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionExecutionEvent {
    Hook {
        animation: u32,
        frame: u32,
        kind: u32,
        payload: MotionHookPayload,
    },
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExecutionCursor {
    pub segment: usize,
    pub frame: f64,
    head: usize,
    completed_clips: usize,
    pub completed: bool,
    initialized: bool,
}
impl Default for ExecutionCursor {
    fn default() -> Self {
        Self {
            segment: 0,
            frame: 0.0,
            head: 0,
            completed_clips: 0,
            completed: false,
            initialized: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionPlaybackEvent {
    pub token: MotionToken,
    pub event: MotionExecutionEvent,
}
#[derive(Clone, Debug)]
pub struct MotionPlayback {
    chain: Arc<PreparedMotionChain>,
    requested: Arc<PreparedMotionChain>,
    cursor: ExecutionCursor,
    pub token: MotionToken,
    completions: Vec<(usize, MotionToken, u32, f32)>,
}
impl PreparedMotionChain {
    pub fn stop_chain(&self) -> Option<&Arc<PreparedMotionChain>> {
        self.stop.as_ref()
    }
    pub fn with_stop_chain(
        mut self,
        stop: Arc<PreparedMotionChain>,
    ) -> Result<Self, TimelineError> {
        if stop.stop.is_some()
            || self.source.map(|s| s.after) != stop.source.map(|s| s.before)
            || self.source.is_none()
        {
            return Err(TimelineError::InvalidRange);
        }
        self.stop = Some(stop);
        Ok(self)
    }
    pub fn source_transition(&self) -> Option<SourceMotionTransition> {
        self.source
    }
    pub fn continues_cycle(&self) -> bool {
        self.source.is_some_and(|s| s.continues_cycle)
    }
    pub fn with_source_transition(
        mut self,
        transition: SourceMotionTransition,
    ) -> Result<Self, TimelineError> {
        for state in [transition.before, transition.after] {
            if state.style == 0
                || state.substate == 0
                || !state.speed.is_finite()
                || state.speed.abs() <= 0.0002
                || state.speed.abs() > 20.0
            {
                return Err(TimelineError::InvalidRate);
            }
        }
        if transition.continues_cycle
            && (self.clips.len() != 1
                || self.completion_clips != 0
                || transition.before.style != transition.after.style
                || transition.before.substate != transition.after.substate
                || transition.before.speed.is_sign_negative()
                    != transition.after.speed.is_sign_negative())
        {
            return Err(TimelineError::InvalidRange);
        }
        self.source = Some(transition);
        Ok(self)
    }
    pub fn is_rootless(&self) -> bool {
        self.physics.velocity == bace_geometry::Vec3::ZERO
            && self.physics.omega == bace_geometry::Vec3::ZERO
            && self.clips.iter().all(|clip| {
                clip.frames
                    .iter()
                    .all(|f| f.translation == bace_geometry::Vec3::ZERO && f.heading == 0.0)
            })
    }
    pub fn cyclic_is_rootless(&self) -> bool {
        self.physics.velocity == bace_geometry::Vec3::ZERO
            && self.physics.omega == bace_geometry::Vec3::ZERO
            && self.clips[self.first_cyclic..].iter().all(|c| {
                c.frames
                    .iter()
                    .all(|f| f.translation == bace_geometry::Vec3::ZERO && f.heading == 0.0)
            })
    }
    pub fn prepare(
        mut clips: Vec<ExecutionClip>,
        first_cyclic: usize,
        completion_clips: usize,
        physics: MotionPhysics,
        motion: u32,
        speed: f32,
    ) -> Result<Self, TimelineError> {
        if clips.is_empty()
            || clips.len() > 512
            || first_cyclic >= clips.len()
            || completion_clips != first_cyclic
            || !physics.velocity.is_finite()
            || !physics.omega.is_finite()
            || !speed.is_finite()
            || speed == 0.0
            || speed.abs() > 20.0
            || motion == 0
        {
            return Err(TimelineError::InvalidRange);
        }
        let mut frames = 0usize;
        let mut hooks = 0usize;
        for (index, clip) in clips.iter_mut().enumerate() {
            if clip.animation == 0
                || clip.frame_count == 0
                || clip.frame_count > 65536
                || clip.low < 0
                || !clip.framerate.is_finite()
                || clip.framerate.abs() <= 0.0002 && index != first_cyclic
                || clip.framerate.abs() > 10000.0
                || !clip.frames.is_empty() && clip.frames.len() != clip.frame_count as usize
            {
                return Err(TimelineError::InvalidRange);
            }
            clip.low = clip.low.min(clip.frame_count as i32 - 1);
            clip.high = if clip.high < 0 {
                clip.frame_count as i32 - 1
            } else {
                clip.high.min(clip.frame_count as i32 - 1)
            }
            .max(clip.low);
            frames = frames
                .checked_add(clip.frames.len())
                .ok_or(TimelineError::Capacity)?;
            hooks = hooks
                .checked_add(clip.hooks.len())
                .ok_or(TimelineError::Capacity)?;
            if frames > 131072
                || hooks > 4096
                || clip
                    .hooks
                    .windows(2)
                    .any(|pair| pair[0].frame > pair[1].frame)
                || clip
                    .hooks
                    .iter()
                    .any(|h| h.frame >= clip.frame_count || ![-2, -1, 0, 1].contains(&h.direction))
                || clip
                    .frames
                    .iter()
                    .any(|f| !f.translation.is_finite() || !f.heading.is_finite())
            {
                return Err(TimelineError::Capacity);
            }
        }
        let max_rate = clips.iter().map(|c| c.framerate.abs()).fold(0.0, f32::max);
        let mut max_hooks = 0usize;
        for clip in &clips {
            let mut frame = None;
            let mut count = 0usize;
            for hook in clip.hooks.iter() {
                if frame != Some(hook.frame) {
                    frame = Some(hook.frame);
                    count = 0;
                }
                count += 1;
                max_hooks = max_hooks.max(count);
            }
        }
        let maximum_crossings =
            (f64::from(max_rate) * f64::from(0.2f32)).ceil() as usize + clips.len();
        if maximum_crossings
            .checked_mul(max_hooks)
            .and_then(|n| n.checked_add(clips.len()))
            .is_none_or(|n| n > 4096)
        {
            return Err(TimelineError::Capacity);
        }
        Ok(Self {
            rate_model: None,
            clips,
            first_cyclic,
            completion_clips,
            physics,
            motion,
            speed,
            source: None,
            stop: None,
        })
    }
    pub fn clips(&self) -> &[ExecutionClip] {
        &self.clips
    }
    pub fn first_cyclic(&self) -> usize {
        self.first_cyclic
    }
    pub fn completion_clips(&self) -> usize {
        self.completion_clips
    }
    pub fn nominal_duration_seconds(&self) -> f64 {
        self.clips[..self.completion_clips]
            .iter()
            .map(|c| {
                let frames = if c.framerate > 0.0 {
                    f64::from(c.high - c.low + 1)
                } else {
                    starting(c) - f64::from(c.low)
                };
                frames / f64::from(c.framerate.abs())
            })
            .sum()
    }
}
impl MotionPlayback {
    pub fn new(chain: Arc<PreparedMotionChain>, token: MotionToken) -> Result<Self, TimelineError> {
        if chain.continues_cycle() {
            return Err(TimelineError::InvalidRange);
        }
        Self::admit(chain, token)
    }
    fn admit(chain: Arc<PreparedMotionChain>, token: MotionToken) -> Result<Self, TimelineError> {
        if token.owner == 0
            || token.sequence == 0
            || chain.motion == 0
            || !chain.speed.is_finite()
            || chain.speed == 0.0
            || chain.speed.abs() > 20.0
            || !chain.physics.velocity.is_finite()
            || !chain.physics.omega.is_finite()
        {
            return Err(TimelineError::InvalidRange);
        }
        let completions = vec![(chain.completion_clips, token, chain.motion, chain.speed)];
        Ok(Self {
            requested: chain.clone(),
            chain,
            cursor: ExecutionCursor::default(),
            token,
            completions,
        })
    }
    /// GDLE same-substate speed update changes cyclic rate, never frame phase.
    /// Only a completed prior transition can be replaced by this bounded owner.
    pub fn continue_cycle(
        &self,
        chain: Arc<PreparedMotionChain>,
        token: MotionToken,
    ) -> Result<Self, TimelineError> {
        let transition = chain.source.ok_or(TimelineError::InvalidRange)?;
        let old = self.chain.source.ok_or(TimelineError::InvalidRange)?;
        if !transition.continues_cycle
            || old.after != transition.before
            || !self.cursor.completed
            || self.cursor.segment != self.chain.first_cyclic
        {
            return Err(TimelineError::InvalidRange);
        }
        let a = &self.chain.clips[self.chain.first_cyclic];
        let b = &chain.clips[0];
        if a.animation != b.animation
            || a.low != b.low
            || a.high != b.high
            || a.frame_count != b.frame_count
            || a.frames != b.frames
            || a.hooks != b.hooks
        {
            return Err(TimelineError::InvalidRange);
        }
        let mut next = Self::admit(chain, token)?;
        // Source multiplies its existing rate (rather than recomputing base*speed).
        let rate = a.framerate * (transition.after.speed / transition.before.speed);
        let mut prepared = (*next.chain).clone();
        prepared.clips[0].framerate = rate;
        if !rate.is_finite() || rate.abs() > 10000.0 {
            return Err(TimelineError::InvalidRate);
        }
        next.chain = Arc::new(prepared);
        next.cursor = ExecutionCursor {
            segment: 0,
            frame: self.cursor.frame,
            head: 0,
            completed_clips: 0,
            completed: false,
            initialized: true,
        };
        Ok(next)
    }
    /// GDLE casting/attack arbitration checks interpreted actions, not every
    /// unfinished transition. Retained Ready/substate links do not block it.
    pub fn has_pending_action(&self) -> bool {
        self.pending_callbacks()
            .any(|(_, _, motion, _)| motion & 0x10000000 != 0)
    }
    pub fn has_ready_zero_callback(&self) -> bool {
        self.pending_callbacks()
            .any(|(at, _, _, _)| *at <= self.cursor.completed_clips)
    }
    fn pending_callbacks(&self) -> impl Iterator<Item = &(usize, MotionToken, u32, f32)> + '_ {
        self.completions.iter().filter(|(at, _, _, _)| {
            *at > self.cursor.completed_clips
                || *at == 0
                    && (!self.cursor.initialized
                        || !self.cursor.completed && self.chain.completion_clips == 0)
        })
    }
    pub fn pending_tokens(&self) -> impl Iterator<Item = MotionToken> + '_ {
        self.pending_callbacks().map(|(_, token, _, _)| *token)
    }
    /// Actual accepted interpreted action requests, preserving their admitted
    /// speed even while later Ready/style links append to the same live cursor.
    pub fn pending_actions(&self) -> impl Iterator<Item = (MotionToken, u32, f32)> + '_ {
        self.pending_callbacks()
            .filter_map(|(_, token, motion, speed)| {
                (motion & 0x10000000 != 0).then_some((*token, *motion, *speed))
            })
    }
    pub fn cursor(&self) -> ExecutionCursor {
        self.cursor
    }
    pub fn chain(&self) -> &Arc<PreparedMotionChain> {
        &self.chain
    }
    pub fn requested_chain(&self) -> &Arc<PreparedMotionChain> {
        &self.requested
    }
    /// Caller supplies reusable output storage. Both capacity and execution are
    /// preflighted before cursor/output mutation; a full sink retains the motion.
    pub fn advance(
        &mut self,
        seconds: f64,
        output: &mut Vec<MotionExecutionEvent>,
        maximum: usize,
    ) -> Result<RootFrame, TimelineError> {
        let mut count = 0usize;
        let (cursor, frame) = self.run_tagged(seconds, |_| count += 1)?;
        if count > maximum.saturating_sub(output.len()) || output.len() + count > output.capacity()
        {
            return Err(TimelineError::Capacity);
        }
        self.run_tagged(seconds, |event| output.push(event.event))?;
        self.cursor = cursor;
        Ok(frame)
    }
    pub fn advance_tagged(
        &mut self,
        seconds: f64,
        output: &mut Vec<MotionPlaybackEvent>,
        maximum: usize,
    ) -> Result<RootFrame, TimelineError> {
        let mut count = 0usize;
        let (cursor, frame) = self.run_tagged(seconds, |_| count += 1)?;
        if count > maximum.saturating_sub(output.len()) || output.len() + count > output.capacity()
        {
            return Err(TimelineError::Capacity);
        }
        self.run_tagged(seconds, |event| output.push(event))?;
        self.cursor = cursor;
        Ok(frame)
    }
    fn run_tagged(
        &self,
        seconds: f64,
        mut emit: impl FnMut(MotionPlaybackEvent),
    ) -> Result<(ExecutionCursor, RootFrame), TimelineError> {
        run(&self.chain, self.cursor, seconds, |event, count| {
            if event == MotionExecutionEvent::Completed {
                for (_, token, _, _) in self.completions.iter().filter(|(at, _, _, _)| *at == count)
                {
                    emit(MotionPlaybackEvent {
                        token: *token,
                        event,
                    });
                }
            } else {
                let token = self
                    .completions
                    .iter()
                    .find(|(at, _, _, _)| *at > count)
                    .map_or(self.token, |(_, token, _, _)| *token);
                emit(MotionPlaybackEvent { token, event });
            }
        })
    }
    /// Retain active links and their completion tokens while replacing only the
    /// cyclic suffix. Same-substate stops retain the cycle phase/rate program.
    pub fn append_stop(
        &self,
        stop: Arc<PreparedMotionChain>,
        token: MotionToken,
    ) -> Result<Self, TimelineError> {
        if self.cursor.completed {
            return if stop.continues_cycle() {
                self.continue_cycle(stop, token)
            } else {
                Self::new(stop, token)
            };
        }
        if self.completions.len() >= 64
            || self.chain.source.map(|s| s.after) != stop.source.map(|s| s.before)
        {
            return Err(TimelineError::Capacity);
        }
        let mut clips = self.chain.clips[..self.chain.first_cyclic].to_vec();
        let prefix = clips.len();
        if stop.continues_cycle() {
            let transition = stop.source.ok_or(TimelineError::InvalidRange)?;
            let mut clip = self.chain.clips[self.chain.first_cyclic].clone();
            let expected = &stop.clips[0];
            if clip.animation != expected.animation
                || clip.low != expected.low
                || clip.high != expected.high
                || clip.frames != expected.frames
                || clip.hooks != expected.hooks
            {
                return Err(TimelineError::InvalidRange);
            }
            clip.framerate *= transition.after.speed / transition.before.speed;
            clips.push(clip);
        } else {
            clips.extend_from_slice(&stop.clips);
        }
        let mut count = prefix
            .checked_add(stop.completion_clips)
            .ok_or(TimelineError::Capacity)?;
        let mut completions = self.completions.clone();
        completions.push((count, token, stop.motion, stop.speed));
        let mut cursor = self.cursor;
        // Original remove_redundant_links: an action/modifier/style is a barrier;
        // repeated nonzero plain substates coalesce later links but retain FIFO
        // zero-count callback nodes behind the earlier matching transition.
        if stop.completion_clips != 0
            && stop.motion & 0x40000000 != 0
            && stop.motion & 0x20000000 == 0
        {
            for index in (0..completions.len() - 1).rev() {
                let (at, _, motion, _) = completions[index];
                let previous = if index == 0 {
                    0
                } else {
                    completions[index - 1].0
                };
                if at <= cursor.completed_clips || at == previous {
                    continue;
                }
                if motion == stop.motion {
                    clips.drain(at..count);
                    for entry in &mut completions[index + 1..] {
                        entry.0 = at;
                    }
                    if cursor.segment >= at {
                        cursor.segment = at;
                        cursor.frame = starting(&clips[at]);
                        cursor.head = cursor.head.min(at);
                    }
                    count = at;
                    break;
                }
                if motion & 0xb0000000 != 0 {
                    break;
                }
            }
        }
        let mut prepared = PreparedMotionChain::prepare(
            clips,
            count,
            count,
            stop.physics,
            stop.motion,
            stop.speed,
        )?;
        prepared.source = stop.source.map(|mut s| {
            s.continues_cycle = false;
            s
        });
        prepared.stop = stop.stop.clone();
        let mut next = Self::new(Arc::new(prepared), token)?;
        next.cursor = cursor;
        next.completions = completions;
        next.requested = stop;
        Ok(next)
    }
}
fn starting(clip: &ExecutionClip) -> f64 {
    if clip.framerate >= 0.0 {
        f64::from(clip.low)
    } else {
        f64::from((clip.high + 1) as f32 - 0.0002)
    }
}
fn position(clip: &ExecutionClip, index: i32) -> RootFrame {
    clip.frames.get(index as usize).copied().unwrap_or_default()
}
fn physics(frame: &mut RootFrame, source: MotionPhysics, quantum: f64) {
    let q = quantum.abs() as f32;
    frame.translation = frame.translation + source.velocity * q;
    frame.heading += source.omega.z * q;
}
fn run(
    chain: &PreparedMotionChain,
    mut cursor: ExecutionCursor,
    seconds: f64,
    mut emit: impl FnMut(MotionExecutionEvent, usize),
) -> Result<(ExecutionCursor, RootFrame), TimelineError> {
    if !seconds.is_finite() || !(0.0..=f64::from(0.2f32)).contains(&seconds) {
        return Err(TimelineError::InvalidRate);
    }
    if !cursor.initialized {
        cursor.frame = starting(&chain.clips[0]);
        cursor.initialized = true;
        // MotionTableManager.CheckForCompletedMotions completes zero-link
        // substates independently of cyclic animation duration.
        emit(MotionExecutionEvent::Completed, 0);
        if chain.completion_clips == 0 {
            cursor.completed = true;
        }
    }
    if chain.completion_clips == 0 && !cursor.completed {
        cursor.completed = true;
        emit(MotionExecutionEvent::Completed, cursor.completed_clips);
    }
    let mut remaining = seconds;
    let mut root = RootFrame::default();
    let mut budget = 4096usize;
    loop {
        budget = budget.checked_sub(1).ok_or(TimelineError::Capacity)?;
        let clip = chain
            .clips
            .get(cursor.segment)
            .ok_or(TimelineError::InvalidRange)?;
        let rate = f64::from(clip.framerate);
        let elapsed = rate * remaining;
        let mut last = cursor.frame.floor() as i32;
        cursor.frame += elapsed;
        let mut extra = 0.0;
        let mut done = false;
        if elapsed > 0.0 {
            if f64::from(clip.high) < cursor.frame.floor() {
                extra = (cursor.frame - f64::from(clip.high) - 1.0).max(0.0) / rate;
                cursor.frame = f64::from(clip.high);
                done = true;
            }
            while cursor.frame.floor() > f64::from(last) {
                budget = budget.checked_sub(1).ok_or(TimelineError::Capacity)?;
                root.combine(position(clip, last));
                physics(&mut root, chain.physics, 1.0 / rate);
                hooks(clip, last, 1, &mut |e| emit(e, cursor.completed_clips));
                last += 1;
            }
        } else if elapsed < 0.0 {
            if f64::from(clip.low) > cursor.frame.floor() {
                extra = (cursor.frame - f64::from(clip.low)).min(0.0) / rate;
                cursor.frame = f64::from(clip.low);
                done = true;
            }
            while cursor.frame.floor() < f64::from(last) {
                budget = budget.checked_sub(1).ok_or(TimelineError::Capacity)?;
                root.subtract(position(clip, last));
                physics(&mut root, chain.physics, 1.0 / rate);
                hooks(clip, last, -1, &mut |e| emit(e, cursor.completed_clips));
                last -= 1;
            }
        } else if remaining > f64::from(0.0002f32) {
            physics(&mut root, chain.physics, remaining);
        }
        if !done {
            break;
        }
        if cursor.head != chain.first_cyclic {
            cursor.completed_clips = cursor
                .completed_clips
                .checked_add(1)
                .ok_or(TimelineError::Capacity)?;
            emit(MotionExecutionEvent::Completed, cursor.completed_clips);
            if cursor.completed_clips >= chain.completion_clips {
                cursor.completed = true;
            }
        }
        if rate < 0.0 {
            root.subtract(position(clip, cursor.frame.floor() as i32));
            physics(&mut root, chain.physics, 1.0 / rate);
        }
        cursor.segment = if cursor.segment + 1 < chain.clips.len() {
            cursor.segment + 1
        } else {
            chain.first_cyclic
        };
        let next = &chain.clips[cursor.segment];
        cursor.frame = starting(next);
        if next.framerate > 0.0 {
            root.combine(position(next, cursor.frame.floor() as i32));
            physics(&mut root, chain.physics, 1.0 / f64::from(next.framerate));
        }
        remaining = extra;
    }
    cursor.head = cursor.segment.min(chain.first_cyclic);
    if !root.translation.is_finite() || !root.heading.is_finite() {
        return Err(TimelineError::InvalidRange);
    }
    Ok((cursor, root))
}
fn hooks(
    clip: &ExecutionClip,
    frame: i32,
    direction: i32,
    emit: &mut impl FnMut(MotionExecutionEvent),
) {
    let start = clip.hooks.partition_point(|h| h.frame < frame as u32);
    for hook in clip.hooks[start..]
        .iter()
        .take_while(|h| h.frame == frame as u32)
        .filter(|h| h.direction == 0 || h.direction == direction)
    {
        emit(MotionExecutionEvent::Hook {
            animation: clip.animation,
            frame: hook.frame,
            kind: hook.kind,
            payload: hook.payload,
        });
    }
}
