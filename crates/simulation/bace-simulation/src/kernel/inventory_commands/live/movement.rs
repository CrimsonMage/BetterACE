//! Source cylinder reach and 1.1m drop displacement, using authoritative sweeps.
use super::*;
use bace_geometry::Vec3;
impl Kernel {
    fn inventory_live_geometry(&self, live: &Live) -> Result<Option<(Vec3, f64, bool)>, E> {
        let actor = live.prepared.evidence.context.actor;
        let Some(target) = live.prepared.evidence.target else {
            return Ok(None);
        };
        let (cell, a) = self.world.actor_state(actor).map_err(|_| E::NotBound)?;
        let (target_cell, b) = self.world.actor_state(target).map_err(|_| E::MissingItem)?;
        let source = self.world.body(actor).map_err(|_| E::NotBound)?;
        let dest = self.world.body(target).map_err(|_| E::MissingItem)?;
        let shape = source.collision_shape().ok_or(E::MissingGeometry)?;
        let target_shape = dest.collision_shape().ok_or(E::MissingGeometry)?;
        let mut position = b.position();
        if cell != target_cell {
            position = position
                + self
                    .world
                    .geometry()
                    .ok_or(E::MissingGeometry)?
                    .frame_offset(cell.0, target_cell.0)
                    .map_err(|_| E::MissingGeometry)?;
        }
        let cylinder =
            |p: Vec3, s: &bace_physics::CollisionShape| bace_inventory::InventoryCylinder {
                position: [p.x, p.y, p.z],
                radius: s.nominal_radius().unwrap_or(s.horizontal_radius()),
                height: s.nominal_height().unwrap_or(s.height()),
            };
        let distance = bace_inventory::inventory_use_distance(
            cylinder(a.position(), shape),
            cylinder(position, target_shape),
        )
        .ok_or(E::InvalidState)?;
        let from = a.position() + Vec3::new(0., 0., shape.height() * 0.5);
        let to = position + Vec3::new(0., 0., target_shape.height() * 0.5);
        let clear = self
            .world
            .segment_clear(cell, from, to, Some(target.0))
            .map_err(|_| E::MissingGeometry)?;
        Ok(Some((position - a.position(), distance, clear)))
    }
    pub(super) fn inventory_live_in_range(&self, live: &Live) -> Result<bool, E> {
        Ok(self
            .inventory_live_geometry(live)?
            .is_none_or(|(_, distance, clear)| {
                distance <= f64::from(live.prepared.use_radius) && clear
            }))
    }
    pub(super) fn inventory_live_approach(&mut self, live: &mut Live) -> Result<bool, E> {
        let actor = live.prepared.evidence.context.actor;
        let Some((offset, distance, clear)) = self.inventory_live_geometry(live)? else {
            return Ok(true);
        };
        let body = self.world.body(actor).map_err(|_| E::NotBound)?;
        let accepted = body.accepted();
        let heading = (-offset.x).atan2(offset.y);
        let delta = bace_motion::heading_delta(accepted.heading_radians(), heading)
            .map_err(|_| E::InvalidState)?;
        let near = distance <= f64::from(live.prepared.use_radius);
        if near && !clear {
            return Err(E::Obstructed);
        }
        if near && delta.abs() <= 0.1 {
            if let Some(control) = live.control.take() {
                self.world.finish_server_move(actor, control);
                self.world
                    .body_mut(actor)
                    .map_err(|_| E::NotBound)?
                    .finish_server_turn(control);
            }
            return Ok(true);
        }
        let turn = bace_motion::TurnIntent::new((delta / 0.25).clamp(-1., 1.))
            .map_err(|_| E::InvalidState)?;
        let direction = if !near && delta.abs() < 0.5 {
            offset * (1. / offset.length_squared().sqrt().max(0.001))
        } else {
            Vec3::ZERO
        };
        let intent =
            bace_motion::MotionIntent::new(direction, false).map_err(|_| E::InvalidState)?;
        let control = match live.control {
            Some(value) => value,
            None => self
                .world
                .next_server_control(actor)
                .map_err(|_| E::Capacity)?,
        };
        if live.control.is_some() {
            self.world
                .continue_server_move(actor, accepted.epoch(), control, intent)
                .map_err(|_| E::InvalidState)?;
            self.world
                .body_mut(actor)
                .map_err(|_| E::NotBound)?
                .continue_server_turn(accepted.epoch(), control, turn)
                .map_err(|_| E::InvalidState)?;
        } else {
            self.world
                .begin_server_move(actor, accepted.epoch(), control, intent)
                .map_err(|_| E::InvalidState)?;
            live.control = Some(control);
            self.world
                .body_mut(actor)
                .map_err(|_| E::NotBound)?
                .begin_server_turn(accepted.epoch(), control, turn)
                .map_err(|_| E::InvalidState)?;
        }
        Ok(false)
    }
    pub(super) fn inventory_live_pickup(&self, live: &Live) -> Result<u32, E> {
        let Some(target) = live.prepared.evidence.target else {
            return Ok(0x40000018);
        };
        let actor = live.prepared.evidence.context.actor;
        let body = self.world.body(actor).map_err(|_| E::NotBound)?;
        let other = self.world.body(target).map_err(|_| E::MissingItem)?;
        let a = body.collision_shape().ok_or(E::MissingGeometry)?;
        let b = other.collision_shape().ok_or(E::MissingGeometry)?;
        let Some((offset, _, _)) = self.inventory_live_geometry(live)? else {
            return Err(E::InvalidState);
        };
        bace_inventory::inventory_pickup_motion(
            body.accepted().position().z,
            a.nominal_height().unwrap_or(a.height()),
            body.accepted().position().z + offset.z,
            b.nominal_height().unwrap_or(b.height()),
            live.prepared.evidence.target_corpse,
        )
        .ok_or(E::InvalidState)
    }
    pub(super) fn inventory_live_drop(
        &self,
        actor: EntityId,
        shape: std::sync::Arc<bace_physics::CollisionShape>,
    ) -> Result<crate::PreparedStackDrop, E> {
        let (cell, accepted) = self.world.actor_state(actor).map_err(|_| E::NotBound)?;
        let geometry = self.world.geometry().ok_or(E::MissingGeometry)?;
        let (sin, cos) = accepted.heading_radians().sin_cos();
        let delta = Vec3::new(-sin * 1.1, cos * 1.1, 0.);
        let moved = geometry
            .move_body(bace_physics::GeometryStep {
                cell: cell.0,
                position: accepted.position(),
                velocity: delta * 10.,
                seconds: 0.1,
                shape: &shape,
                dynamics: &[],
                ignore: actor.0,
                was_grounded: accepted.grounded(),
                player_status: None,
                allowed_restrictions: &[],
            })
            .map_err(|_| E::MissingGeometry)?;
        self.world
            .validate_player_cell_entry(actor, bace_types::CellId(moved.cell))
            .map_err(|_| E::AccessDenied)?;
        Ok(crate::PreparedStackDrop {
            source_epoch: accepted.epoch(),
            spawn: bace_physics::GeometrySpawn {
                cell: moved.cell,
                position: moved.position,
                shape,
                capabilities: bace_motion::Capabilities {
                    speed: 0.,
                    jump_impulse: 0.,
                },
                heading: accepted.heading_radians(),
                maximum_turn_rate: 1.,
            },
        })
    }
}
