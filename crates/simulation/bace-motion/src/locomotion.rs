//! Source-shaped locomotion interpretation from ACE MotionInterp.adjust_motion
//! and apply_run_to_command (47edade3). Speeds originate in prepared DAT cycles.
use crate::MotionError;
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionPhysics {
    pub velocity: Vec3,
    pub omega: Vec3,
}
#[derive(Clone, Debug)]
pub struct LocomotionProfile {
    pub style: u32,
    pub ready: MotionPhysics,
    pub walk: MotionPhysics,
    pub run: MotionPhysics,
    pub sidestep: MotionPhysics,
    pub turn: MotionPhysics,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct LocomotionControls {
    pub forward: f32,
    pub sidestep: f32,
    pub turn: f32,
    pub run: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocomotionAxes {
    pub forward: f32,
    pub sidestep: f32,
    pub turn: f32,
    pub forward_scale: f32,
    pub forward_run: bool,
    pub sidestep_run: bool,
    pub turn_run: bool,
}
impl Default for LocomotionAxes {
    fn default() -> Self {
        Self {
            forward: 0.0,
            sidestep: 0.0,
            turn: 0.0,
            forward_scale: 1.0,
            forward_run: false,
            sidestep_run: false,
            turn_run: false,
        }
    }
}
impl From<LocomotionControls> for LocomotionAxes {
    fn from(c: LocomotionControls) -> Self {
        Self {
            forward: c.forward,
            sidestep: c.sidestep,
            turn: c.turn,
            forward_scale: if c.forward < 0.0 { 0.65 } else { 1.0 },
            forward_run: c.run,
            sidestep_run: c.run,
            turn_run: c.run,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionDrive {
    pub local_velocity: Vec3,
    pub angular_velocity: f32,
    pub forward_motion: u32,
    pub forward_rate: f32,
    pub side_rate: f32,
    pub turn_rate: f32,
}
impl LocomotionProfile {
    pub fn interpret(
        &self,
        controls: LocomotionControls,
        run_rate: f32,
    ) -> Result<MotionDrive, MotionError> {
        self.interpret_axes(controls.into(), run_rate)
    }
    pub fn interpret_axes(
        &self,
        controls: LocomotionAxes,
        run_rate: f32,
    ) -> Result<MotionDrive, MotionError> {
        if [controls.forward, controls.sidestep, controls.turn]
            .into_iter()
            .any(|v| !v.is_finite() || !(-1.0..=1.0).contains(&v))
            || !run_rate.is_finite()
            || !(0.0..=20.0).contains(&run_rate)
        {
            return Err(MotionError::Capabilities);
        }
        for motion in [self.ready, self.walk, self.run, self.sidestep, self.turn] {
            if !motion.velocity.is_finite() || !motion.omega.is_finite() {
                return Err(MotionError::NonFinite);
            }
        }
        if ![0.65, 1.0].contains(&controls.forward_scale) {
            return Err(MotionError::Capabilities);
        }
        let forward = controls.forward * controls.forward_scale;
        let forward_rate = if forward == 0.0 {
            1.0
        } else {
            forward * if controls.forward_run { run_rate } else { 1.0 }
        };
        let (base, forward_motion) = if controls.forward == 0.0 {
            (self.ready, 0x41000003)
        } else if controls.forward_run && controls.forward > 0.0 {
            (self.run, 0x44000007)
        } else {
            (self.walk, 0x45000005)
        };
        let side_base = controls.sidestep * (0.5 * (3.12 / 1.25));
        let side_rate = if controls.sidestep_run {
            (side_base * run_rate).clamp(-3.0, 3.0)
        } else {
            side_base
        };
        let turn_rate = controls.turn * if controls.turn_run { 1.5 } else { 1.0 };
        let velocity = base.velocity
            * if controls.forward == 0.0 {
                1.0
            } else {
                forward_rate
            }
            + self.sidestep.velocity * side_rate
            + self.turn.velocity * turn_rate;
        let angular = base.omega.z * forward_rate
            + self.sidestep.omega.z * side_rate
            + self.turn.omega.z * turn_rate;
        if !velocity.is_finite()
            || velocity.length_squared() > 2500.0
            || !angular.is_finite()
            || angular.abs() > 20.0
        {
            return Err(MotionError::Capabilities);
        }
        Ok(MotionDrive {
            local_velocity: velocity,
            angular_velocity: angular,
            forward_motion,
            forward_rate,
            side_rate,
            turn_rate,
        })
    }
}
