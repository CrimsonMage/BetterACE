//! Immutable animation playback timelines. Source frame ranges are half-open;
//! negative rates play in reverse. No wall clock, asset I/O or gameplay mutation.
use bace_geometry::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionHookPayload {
    Attack {
        part: u32,
        left: [f32; 2],
        right: [f32; 2],
        radius: f32,
        height: f32,
    },
    State(i32),
    Id(u32),
    Empty,
    ReplaceObject {
        raw_part_index: u16,
        part_index: u8,
        object_id: u32,
    },
    Transition {
        part: Option<u32>,
        start: f32,
        end: f32,
        time: f32,
    },
    Scale {
        end: f32,
        time: f32,
    },
    Particle {
        emitter_info_id: u32,
        part_index: u32,
        origin: [f32; 3],
        rotation: [f32; 4],
        emitter_id: u32,
    },
    CallPes {
        pes: u32,
        pause: f32,
    },
    SoundTweaked {
        sound_id: u32,
        priority: f32,
        probability: f32,
        volume: f32,
    },
    Omega([f32; 3]),
    TextureVelocity {
        part: Option<u32>,
        uv: [f32; 2],
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct MotionHook {
    pub frame: u32,
    pub kind: u32,
    pub direction: i32,
    pub payload: MotionHookPayload,
}
#[derive(Clone, Debug)]
pub struct MotionSegment {
    pub animation: u32,
    pub frame_count: u32,
    pub low_frame: i32,
    pub high_frame: i32,
    pub frames_per_second: f32,
    pub hooks: Vec<MotionHook>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TimedMotionHook {
    pub seconds: f64,
    pub animation: u32,
    pub frame: u32,
    pub kind: u32,
    pub payload: MotionHookPayload,
}
#[derive(Clone, Debug)]
pub struct PreparedMotion {
    duration: f64,
    hooks: Vec<TimedMotionHook>,
    pub velocity: Vec3,
    pub omega: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineError {
    InvalidRange,
    InvalidRate,
    InvalidHook,
    Capacity,
}
impl PreparedMotion {
    pub fn prepare(
        segments: &[MotionSegment],
        velocity: Vec3,
        omega: Vec3,
    ) -> Result<Self, TimelineError> {
        if segments.is_empty()
            || segments.len() > 255
            || !velocity.is_finite()
            || !omega.is_finite()
        {
            return Err(TimelineError::Capacity);
        }
        let mut duration = 0.0;
        let mut hooks = Vec::new();
        for segment in segments {
            if segment.frame_count > 65536 || segment.low_frame < 0 {
                return Err(TimelineError::InvalidRange);
            }
            let high = if segment.high_frame == -1 {
                segment.frame_count
            } else {
                u32::try_from(segment.high_frame)
                    .map_err(|_| TimelineError::InvalidRange)?
                    .min(segment.frame_count)
            };
            let low = segment.low_frame as u32;
            if low >= high || segment.hooks.len() > 4096 {
                return Err(TimelineError::InvalidRange);
            }
            let rate = segment.frames_per_second;
            if !rate.is_finite() || rate == 0.0 {
                return Err(TimelineError::InvalidRate);
            }
            for hook in &segment.hooks {
                if hook.frame >= segment.frame_count || ![-1, 0, 1].contains(&hook.direction) {
                    return Err(TimelineError::InvalidHook);
                }
                if hook.frame < low
                    || hook.frame >= high
                    || hook.direction != 0 && (hook.direction > 0) != (rate > 0.0)
                {
                    continue;
                }
                if hooks.len() == 4096 {
                    return Err(TimelineError::Capacity);
                }
                let offset = if rate > 0.0 {
                    hook.frame - low
                } else {
                    high - 1 - hook.frame
                };
                hooks.push(TimedMotionHook {
                    seconds: duration + f64::from(offset) / f64::from(rate.abs()),
                    animation: segment.animation,
                    frame: hook.frame,
                    kind: hook.kind,
                    payload: hook.payload,
                });
            }
            duration += f64::from(high - low) / f64::from(rate.abs());
            if !duration.is_finite() || duration > 3600.0 {
                return Err(TimelineError::Capacity);
            }
        }
        hooks.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        Ok(Self {
            duration,
            hooks,
            velocity,
            omega,
        })
    }
    pub fn duration_seconds(&self) -> f64 {
        self.duration
    }
    pub fn hooks(&self) -> &[TimedMotionHook] {
        &self.hooks
    }
    pub fn cursor(&self) -> MotionCursor {
        MotionCursor {
            elapsed: 0.0,
            next: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct MotionCursor {
    elapsed: f64,
    next: usize,
}
impl MotionCursor {
    /// A caller preflights output capacity using due_count before advancing, so
    /// every authored hook is emitted once even when downstream queues are full.
    pub fn due_count(&self, motion: &PreparedMotion, seconds: f64) -> Result<usize, TimelineError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(TimelineError::InvalidRate);
        }
        let end = (self.elapsed + seconds).min(motion.duration);
        Ok(motion.hooks[self.next..]
            .iter()
            .take_while(|h| h.seconds <= end)
            .count())
    }
    pub fn advance<'a>(
        &mut self,
        motion: &'a PreparedMotion,
        seconds: f64,
        capacity: usize,
    ) -> Result<&'a [TimedMotionHook], TimelineError> {
        let count = self.due_count(motion, seconds)?;
        if count > capacity {
            return Err(TimelineError::Capacity);
        }
        let start = self.next;
        self.next += count;
        self.elapsed = (self.elapsed + seconds).min(motion.duration);
        Ok(&motion.hooks[start..self.next])
    }
    pub fn finished(&self, motion: &PreparedMotion) -> bool {
        self.elapsed >= motion.duration && self.next == motion.hooks.len()
    }
}
