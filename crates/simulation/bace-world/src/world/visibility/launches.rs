//! Immutable launch evidence uses the existing cell membership index. All PVS
//! links are local to a block; outside bridges add at most its eight neighbours.
use super::*;
use bace_gameplay_api::{
    CharacterBinding,
    visibility::{AcceptedProjectileLaunch, ProjectileLaunchObserver},
};
impl World {
    pub fn projectile_launch_snapshot(
        &mut self,
        projectile: EntityId,
        tick: u64,
        binding: impl Fn(EntityId) -> Option<CharacterBinding>,
    ) -> Result<AcceptedProjectileLaunch, VisibilityError> {
        if !self.projectiles.contains_key(&projectile) {
            return Err(VisibilityError::MissingActor);
        }
        self.object_birth_snapshot(projectile, tick, binding)
    }
    pub fn object_birth_snapshot(
        &mut self,
        projectile: EntityId,
        tick: u64,
        binding: impl Fn(EntityId) -> Option<CharacterBinding>,
    ) -> Result<AcceptedProjectileLaunch, VisibilityError> {
        let view = self
            .accepted_object_view(projectile)
            .map_err(|_| VisibilityError::MissingActor)?;
        self.birth_with_view(view, tick, binding)
    }
    /// Only accepts a physics-created staged Body; caller publishes that same
    /// actor after all birth/PVS preflight succeeds. No supplied pose is trusted.
    pub fn staged_object_birth_snapshot(
        &mut self,
        actor: &Actor,
        tick: u64,
        binding: impl Fn(EntityId) -> Option<CharacterBinding>,
    ) -> Result<AcceptedProjectileLaunch, VisibilityError> {
        if self.contains_identity(actor.id)
            || actor.body.server_move().is_some()
            || actor.body.server_turn().is_some()
        {
            return Err(VisibilityError::InvalidMetadata);
        }
        let state = actor.body.accepted();
        let p = state.position();
        let v = state.velocity();
        let motion = match actor.body.locomotion_projection() {
            Some((style, drive)) => Some(bace_gameplay_api::visibility::AcceptedObjectMotion {
                autonomous: actor.body.locomotion_autonomous(),
                style,
                forward_motion: drive.forward_motion,
                forward_rate: drive.forward_rate,
                sidestep_rate: drive.side_rate,
                turn_rate: drive.turn_rate,
                actions: vec![],
                goal: None,
            }),
            None if actor.body.has_locomotion_intent() => {
                return Err(VisibilityError::InvalidMetadata);
            }
            None => None,
        };
        let view = bace_gameplay_api::visibility::AcceptedObjectView {
            entity: actor.id,
            cell: actor.cell.0,
            position: [p.x, p.y, p.z],
            velocity: [v.x, v.y, v.z],
            heading_radians: state.heading_radians(),
            grounded: state.grounded(),
            epoch: state.epoch(),
            held: false,
            motion: Ok(motion),
        };
        self.birth_with_view(view, tick, binding)
    }
    fn birth_with_view(
        &mut self,
        view: bace_gameplay_api::visibility::AcceptedObjectView,
        tick: u64,
        binding: impl Fn(EntityId) -> Option<CharacterBinding>,
    ) -> Result<AcceptedProjectileLaunch, VisibilityError> {
        if view.motion.is_err() {
            return Err(VisibilityError::InvalidMetadata);
        }
        self.visibility.refresh(&self.actors, &self.projectiles)?;
        let target = CellId(view.cell);
        // Missing target PVS must reject before publishing a partial audience.
        self.visibility.outside(target)?;
        let x = (target.0 >> 24) as i32;
        let y = ((target.0 >> 16) & 255) as i32;
        let mut observers = Vec::new();
        for nx in (x - 1).max(0)..=(x + 1).min(255) {
            for ny in (y - 1).max(0)..=(y + 1).min(255) {
                let start = CellId(((nx as u32) << 24) | ((ny as u32) << 16));
                let end = CellId(start.0 | 0xffff);
                for (cell, ids) in self.visibility.by_cell.range(start..=end) {
                    if !self.cells_share_visibility(*cell, target)? {
                        continue;
                    }
                    for &id in ids {
                        let Some(bound) = binding(id) else {
                            continue;
                        };
                        if bound.actor != id {
                            return Err(VisibilityError::InvalidMetadata);
                        }
                        if self.is_in_portal_transit(id) || self.entry_pending.contains(&id) {
                            continue;
                        }
                        let Some(actor) = self.actors.get(&id) else {
                            continue;
                        };
                        if observers.len() >= 4096 {
                            return Err(VisibilityError::Capacity);
                        }
                        let state = actor.body.accepted();
                        observers.push(ProjectileLaunchObserver {
                            binding: bound,
                            epoch: state.epoch(),
                            distance_squared: visibility_distance_squared(
                                *cell,
                                state.position(),
                                target,
                                Vec3::new(view.position[0], view.position[1], view.position[2]),
                            )?,
                        });
                    }
                }
            }
        }
        observers.sort_unstable_by_key(|o| o.binding.actor);
        Ok(AcceptedProjectileLaunch {
            tick,
            view,
            observers,
        })
    }
}
