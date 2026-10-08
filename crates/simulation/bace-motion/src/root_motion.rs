//! Bounded upright root-animation playback. Frame stepping follows pinned ACE
//! Sequence.update_internal/advance_to_next_animation; no clock or asset access.
use crate::{LocomotionProfile, MotionDrive, TimelineError};
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RootFrame {
    pub translation: Vec3,
    pub heading: f32,
}
impl RootFrame {
    pub(crate) fn combine(&mut self, other: Self) {
        self.translation = self.translation + rotate(other.translation, self.heading);
        self.heading += other.heading;
    }
    pub(crate) fn subtract(&mut self, other: Self) {
        self.translation = self.translation - rotate(other.translation, other.heading);
        self.heading -= other.heading;
    }
}
fn rotate(v: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}
#[derive(Clone, Debug)]
pub struct RootSegment {
    pub low: i32,
    pub high: i32,
    pub frame_count: u32,
    pub framerate: f32,
    pub frames: Vec<RootFrame>,
}
#[derive(Clone, Debug)]
pub struct RootCycle {
    segments: Vec<RootSegment>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct RootCursor {
    segment: usize,
    frame: f32,
    initialized: bool,
    negative: bool,
}
impl RootCursor {
    pub fn phase(&self) -> (usize, f32) {
        (self.segment, self.frame)
    }
}
impl RootCycle {
    pub fn prepare(mut segments: Vec<RootSegment>) -> Result<Self, TimelineError> {
        if segments.is_empty() || segments.len() > 255 {
            return Err(TimelineError::Capacity);
        }
        let mut count = 0usize;
        for segment in &mut segments {
            if segment.frame_count == 0
                || segment.frame_count > 65536
                || segment.low < 0
                || !segment.framerate.is_finite()
                || segment.framerate.abs() > 10000.0
            {
                return Err(TimelineError::InvalidRange);
            }
            if !segment.frames.is_empty() && segment.frames.len() != segment.frame_count as usize {
                return Err(TimelineError::InvalidRange);
            }
            if segment
                .frames
                .iter()
                .any(|f| !f.translation.is_finite() || !f.heading.is_finite())
            {
                return Err(TimelineError::InvalidRange);
            }
            count = count
                .checked_add(segment.frames.len())
                .ok_or(TimelineError::Capacity)?;
            if count > 65536 {
                return Err(TimelineError::Capacity);
            }
            segment.low = segment.low.min(segment.frame_count as i32 - 1);
            segment.high = if segment.high < 0 {
                segment.frame_count as i32 - 1
            } else {
                segment.high.min(segment.frame_count as i32 - 1)
            };
            segment.high = segment.high.max(segment.low);
        }
        Ok(Self { segments })
    }
    /// Atomic cursor advancement. Unlike a guessed average velocity, the actual
    /// authored position frame contributes only when the source frame advances.
    pub fn advance(
        &self,
        cursor: &mut RootCursor,
        seconds: f32,
        speed: f32,
        drive: MotionDrive,
    ) -> Result<RootFrame, TimelineError> {
        if !seconds.is_finite()
            || !(0.0..=0.1).contains(&seconds)
            || !speed.is_finite()
            || speed.abs() > 20.0
        {
            return Err(TimelineError::InvalidRate);
        }
        if cursor.initialized
            && (cursor.segment >= self.segments.len() || !cursor.frame.is_finite())
        {
            return Err(TimelineError::InvalidRange);
        }
        let mut next = *cursor;
        let mut result = RootFrame::default();
        let mut remaining = seconds;
        let mut budget = 1024usize;
        if !next.initialized || next.negative != (speed < 0.0) {
            next = RootCursor {
                segment: 0,
                frame: starting(&self.segments[0], speed),
                initialized: true,
                negative: speed < 0.0,
            };
        }
        loop {
            budget = budget.checked_sub(1).ok_or(TimelineError::Capacity)?;
            let segment = &self.segments[next.segment];
            let rate = segment.framerate * speed;
            let elapsed = rate * remaining;
            let mut last = next.frame.floor() as i32;
            next.frame += elapsed;
            let mut extra = 0.0;
            let mut done = false;
            if elapsed > 0.0 {
                if (segment.high as f32) < next.frame.floor() {
                    extra = ((next.frame - segment.high as f32 - 1.0).max(0.0)) / rate;
                    next.frame = segment.high as f32;
                    done = true;
                }
                while next.frame.floor() > last as f32 {
                    budget = budget.checked_sub(1).ok_or(TimelineError::Capacity)?;
                    result.combine(position(segment, last));
                    apply_physics(&mut result, drive, 1.0 / rate, remaining);
                    last += 1;
                }
            } else if elapsed < 0.0 {
                if (segment.low as f32) > next.frame.floor() {
                    extra = ((next.frame - segment.low as f32).min(0.0)) / rate;
                    next.frame = segment.low as f32;
                    done = true;
                }
                while next.frame.floor() < last as f32 {
                    budget = budget.checked_sub(1).ok_or(TimelineError::Capacity)?;
                    result.subtract(position(segment, last));
                    apply_physics(&mut result, drive, 1.0 / rate, remaining);
                    last -= 1;
                }
            } else {
                apply_physics(&mut result, drive, remaining, remaining);
            }
            if !done {
                break;
            }
            // Forward simulation time advances to the next cyclic segment even
            // for reverse-playing source animations (Sequence's source order).
            if rate < 0.0 {
                result.subtract(position(segment, next.frame as i32));
                apply_physics(&mut result, drive, 1.0 / rate, remaining);
            }
            // append_animation makes the last node FirstCyclic. Earlier nodes
            // are a one-time warmup, not a repeating multi-clip cycle.
            next.segment = (next.segment + 1).min(self.segments.len() - 1);
            let segment = &self.segments[next.segment];
            next.frame = starting(segment, speed);
            let rate = segment.framerate * speed;
            if rate > 0.0 {
                result.combine(position(segment, next.frame as i32));
                apply_physics(&mut result, drive, 1.0 / rate, remaining);
            }
            remaining = extra;
        }
        if !result.translation.is_finite() || !result.heading.is_finite() {
            return Err(TimelineError::InvalidRange);
        }
        *cursor = next;
        Ok(result)
    }
}
fn starting(segment: &RootSegment, speed: f32) -> f32 {
    if segment.framerate * speed >= 0.0 {
        segment.low as f32
    } else {
        segment.high as f32 + 1.0 - 0.0002
    }
}
fn position(segment: &RootSegment, frame: i32) -> RootFrame {
    segment
        .frames
        .get(frame as usize)
        .copied()
        .unwrap_or_default()
}
fn apply_physics(frame: &mut RootFrame, drive: MotionDrive, quantum: f32, sign: f32) {
    let quantum = if sign >= 0.0 {
        quantum.abs()
    } else {
        -quantum.abs()
    };
    frame.translation = frame.translation + drive.local_velocity * quantum;
    frame.heading += drive.angular_velocity * quantum;
}
#[derive(Clone, Debug)]
pub struct AnimatedLocomotion {
    pub profile: LocomotionProfile,
    pub ready: RootCycle,
    pub walk: RootCycle,
    pub run: RootCycle,
}
impl AnimatedLocomotion {
    pub fn cycle(&self, motion: u32) -> &RootCycle {
        match motion {
            0x44000007 => &self.run,
            0x45000005 => &self.walk,
            _ => &self.ready,
        }
    }
}
