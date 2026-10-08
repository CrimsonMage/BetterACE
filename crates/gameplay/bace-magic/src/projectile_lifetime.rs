//! GDLE SpellProjectile.cpp Tick and collision cleanup deadlines. Explicit time;
//! first explosion disables further impacts and cannot be extended by duplicates.
use crate::ProjectileLayoutError as E;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileLifeAction {
    None,
    Explode,
    Destroy,
}
#[derive(Clone, Copy, Debug)]
pub struct SpellProjectileLifetime {
    spawned_at: f64,
    last_time: f64,
    destroy_at: Option<f64>,
    destroyed: bool,
}
impl SpellProjectileLifetime {
    pub fn new(now: f64) -> Result<Self, E> {
        if !now.is_finite() || now < 0.0 || now + 30.0 <= now {
            return Err(E::InvalidGeometry);
        }
        Ok(Self {
            spawned_at: now,
            last_time: now,
            destroy_at: None,
            destroyed: false,
        })
    }
    pub fn exploded(&self) -> bool {
        self.destroy_at.is_some()
    }
    pub fn spawned_at(&self) -> f64 {
        self.spawned_at
    }
    fn time(&self, now: f64) -> Result<(), E> {
        if !now.is_finite() || now < self.last_time || now + 1.0 <= now {
            Err(E::InvalidGeometry)
        } else {
            Ok(())
        }
    }
    pub fn collide(&mut self, now: f64, damage_target: bool) -> Result<ProjectileLifeAction, E> {
        self.time(now)?;
        self.last_time = now;
        if self.destroyed || self.destroy_at.is_some() {
            return Ok(ProjectileLifeAction::None);
        }
        self.destroy_at = Some(now + if damage_target { 0.5 } else { 1.0 });
        Ok(ProjectileLifeAction::Explode)
    }
    pub fn tick(
        &mut self,
        now: f64,
        valid_cell: bool,
        distance: f32,
        maximum_range: f32,
    ) -> Result<ProjectileLifeAction, E> {
        self.time(now)?;
        if !distance.is_finite()
            || distance < 0.0
            || !maximum_range.is_finite()
            || maximum_range < 0.0
        {
            return Err(E::InvalidGeometry);
        }
        self.last_time = now;
        if self.destroyed {
            return Ok(ProjectileLifeAction::None);
        }
        if !valid_cell || self.destroy_at.is_some_and(|end| end <= now) {
            self.destroyed = true;
            return Ok(ProjectileLifeAction::Destroy);
        }
        if self.destroy_at.is_none() && (distance > maximum_range || now >= self.spawned_at + 29.0)
        {
            self.destroy_at = Some(now + 1.0);
            return Ok(ProjectileLifeAction::Explode);
        }
        Ok(ProjectileLifeAction::None)
    }
}
