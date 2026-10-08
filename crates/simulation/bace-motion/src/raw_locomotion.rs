//! ACE RawMotionState defaults + MotionInterp.adjust_motion hold-key selection.
//! Raw RunForward is never installed by RawMotionState.ApplyMotion; clients use
//! WalkForward + HoldKey.Run. Normalized speeds are legal control strength only.
use crate::{LocomotionAxes, MotionError};
#[derive(Clone, Copy, Debug)]
pub struct RawLocomotion {
    pub current_hold: u32,
    pub forward: (u32, u32, f32),
    pub sidestep: (u32, u32, f32),
    pub turn: (u32, u32, f32),
}
pub fn interpret_raw_controls(raw: RawLocomotion) -> Result<LocomotionAxes, MotionError> {
    if raw.current_hold > 2 {
        return Err(MotionError::Capabilities);
    }
    let axis = |(command, hold, speed): (u32, u32, f32),
                positive,
                negative,
                ready|
     -> Result<(f32, bool), MotionError> {
        if hold > 2 || !speed.is_finite() || !(-1.0..=1.0).contains(&speed) {
            return Err(MotionError::Capabilities);
        }
        let value = if command == positive {
            speed
        } else if command == negative {
            -speed
        } else if command == 0 || command == ready {
            0.0
        } else {
            return Err(MotionError::Capabilities);
        };
        Ok((
            value,
            if hold == 0 {
                raw.current_hold == 2
            } else {
                hold == 2
            },
        ))
    };
    let (forward, forward_run) = axis(raw.forward, 0x45000005, 0x45000006, 0x41000003)?;
    let (sidestep, sidestep_run) = axis(raw.sidestep, 0x6500000f, 0x65000010, 0)?;
    let (turn, turn_run) = axis(raw.turn, 0x6500000d, 0x6500000e, 0)?;
    Ok(LocomotionAxes {
        forward,
        sidestep,
        turn,
        forward_scale: if raw.forward.0 == 0x45000006 {
            0.65
        } else {
            1.0
        },
        forward_run,
        sidestep_run,
        turn_run,
    })
}
