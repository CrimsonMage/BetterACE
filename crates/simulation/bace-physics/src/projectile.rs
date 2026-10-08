//! Bounded scalar projectile integration. Collision comes from the owning world,
//! never a client hit report. Authentic AC trajectory/transition qualification is
//! separate from this checked same-cell integration boundary.
use crate::PhysicsError;
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileHit {
    pub fraction: f32,
    pub target: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProjectileStep {
    Flying,
    Impact { target: Option<u32> },
    Expired,
    Finished,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileBody {
    position: Vec3,
    velocity: Vec3,
    radius: f32,
    gravity: f32,
    age: f64,
    lifetime: f64,
    finished: bool,
}
impl ProjectileBody {
    pub fn new(
        position: Vec3,
        velocity: Vec3,
        radius: f32,
        gravity: f32,
        lifetime: f64,
    ) -> Result<Self, PhysicsError> {
        if !position.is_finite()
            || !velocity.is_finite()
            || velocity.length_squared() > 1_000_000.0
            || !radius.is_finite()
            || !(0.0..=20.0).contains(&radius)
            || radius == 0.0
            || !gravity.is_finite()
            || gravity.abs() > 100.0
            || !lifetime.is_finite()
            || !(0.0..=600.0).contains(&lifetime)
            || lifetime == 0.0
        {
            return Err(PhysicsError::InvalidState);
        }
        Ok(Self {
            position,
            velocity: if [velocity.x, velocity.y, velocity.z]
                .into_iter()
                .all(|v| v.abs() < 0.0002)
            {
                Vec3::ZERO
            } else {
                limit_velocity(velocity)
            },
            radius,
            gravity,
            age: 0.0,
            lifetime,
            finished: false,
        })
    }
    /// World-owned coordinate-frame transition after a validated portal trace.
    pub fn translate_frame(&mut self, translation: Vec3) -> Result<(), PhysicsError> {
        let position = self.position + translation;
        if !translation.is_finite() || !position.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        self.position = position;
        Ok(())
    }
    pub fn position(&self) -> Vec3 {
        self.position
    }
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }
    pub fn radius(&self) -> f32 {
        self.radius
    }
    pub fn finished(&self) -> bool {
        self.finished
    }
    pub fn endpoint(&self, seconds: f32) -> Result<Vec3, PhysicsError> {
        if !seconds.is_finite() || !(0.0..=0.2).contains(&seconds) || seconds == 0.0 {
            return Err(PhysicsError::InvalidState);
        }
        let dt = f64::from(seconds).min((self.lifetime - self.age).max(0.0)) as f32;
        let (endpoint, _) = free_flight(self.position, self.velocity, self.gravity, dt);
        if !endpoint.is_finite() {
            return Err(PhysicsError::InvalidState);
        }
        Ok(endpoint)
    }
    pub fn step(
        &mut self,
        seconds: f32,
        hit: Option<ProjectileHit>,
    ) -> Result<ProjectileStep, PhysicsError> {
        let endpoint = self.endpoint(seconds)?;
        if self.finished {
            return Ok(ProjectileStep::Finished);
        }
        if hit.is_some_and(|h| !h.fraction.is_finite() || !(0.0..=1.0).contains(&h.fraction)) {
            return Err(PhysicsError::InvalidState);
        }
        let elapsed = f64::from(seconds).min((self.lifetime - self.age).max(0.0));
        let dt = elapsed as f32;
        if let Some(hit) = hit {
            self.position = self.position + (endpoint - self.position) * hit.fraction;
            self.velocity = Vec3::ZERO;
            self.finished = true;
            return Ok(ProjectileStep::Impact { target: hit.target });
        }
        let (_, velocity) = free_flight(self.position, self.velocity, self.gravity, dt);
        self.position = endpoint;
        self.velocity = velocity;
        self.age += elapsed;
        if self.age >= self.lifetime {
            self.finished = true;
            Ok(ProjectileStep::Expired)
        } else {
            Ok(ProjectileStep::Flying)
        }
    }
}
/// Pinned GDLE CPhysicsObj::set_velocity and airborne UpdatePhysicsInternal.
/// This scalar contract excludes ground friction and orientation. Retail client
/// decompilation corroborates the branches, not f32/x87 numeric equivalence.
fn limit_velocity(mut velocity: Vec3) -> Vec3 {
    let squared = velocity.length_squared();
    if squared > 2500.0 {
        velocity = velocity * (1.0 / squared.sqrt());
        velocity = velocity * 50.0;
    }
    velocity
}
fn free_flight(position: Vec3, mut velocity: Vec3, gravity: f32, seconds: f32) -> (Vec3, Vec3) {
    let mut position = position;
    let mut squared = velocity.length_squared();
    if squared > 0.0 {
        if squared > 2500.0 {
            velocity = limit_velocity(velocity);
            squared = 2500.0;
        }
        if squared - 0.25 * 0.25 < 0.0002 {
            velocity = Vec3::ZERO;
        }
        let acceleration = Vec3::new(0.0, 0.0, gravity);
        let change = acceleration * 0.5 * seconds * seconds + velocity * seconds;
        position = position + change;
    }
    velocity.z += gravity * seconds;
    (position, velocity)
}
