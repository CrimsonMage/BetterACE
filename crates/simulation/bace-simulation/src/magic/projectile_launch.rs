//! Prepared launch geometry from the same World's accepted bodies.
use super::*;
pub(super) fn actor_frame(
    world: &World,
    actor: EntityId,
    in_cell: CellId,
) -> Result<bace_magic::ProjectileActorFrame, CastRejection> {
    let (position, velocity) = world
        .actor_in_frame(actor, in_cell)
        .map_err(|_| CastRejection::MissingAssets)?;
    let body = world.body(actor).map_err(|_| CastRejection::MissingActor)?;
    let (position, height, radius) = if let Some(shape) = body.collision_shape() {
        (
            position,
            shape.nominal_height().ok_or(CastRejection::MissingAssets)?,
            shape.nominal_radius().ok_or(CastRejection::MissingAssets)?,
        )
    } else {
        let radius = body.collision_radius();
        (position - Vec3::new(0.0, 0.0, radius), radius * 2.0, radius)
    };
    Ok(bace_magic::ProjectileActorFrame {
        position,
        height,
        radius,
        heading: body.accepted().heading_radians(),
        velocity,
    })
}
pub(super) fn trajectories(
    actor: EntityId,
    target: Option<EntityId>,
    spec: &ProjectileSpec,
    random: &RandomStream,
    world: &World,
) -> Result<(CellId, Vec<bace_magic::ProjectileLaunch>), CastRejection> {
    if matches!(
        spec.shape,
        bace_magic::ProjectileShape::Bolt
            | bace_magic::ProjectileShape::Streak
            | bace_magic::ProjectileShape::Arc
    ) && spec.count == 1
        && spec.dimensions == [1, 1, 1]
        && spec.spread_degrees == 0.0
    {
        let target = target.ok_or(CastRejection::InvalidTarget)?;
        let (cell, _) = world
            .actor_state(actor)
            .map_err(|_| CastRejection::MissingActor)?;
        let source = actor_frame(world, actor, cell)?;
        let target_frame = actor_frame(world, target, cell)?;
        let draw = if spec.perturbation == Vec3::ZERO {
            Vec3::ZERO
        } else {
            let mut stream = random
                .fork(b"origins", 0)
                .map_err(|_| CastRejection::InvalidState)?;
            Vec3::new(
                unit(&mut stream)? * 2.0 - 1.0,
                unit(&mut stream)? * 2.0 - 1.0,
                unit(&mut stream)? * 2.0 - 1.0,
            )
        };
        return bace_magic::gdle_single_projectile(
            spec,
            source,
            target_frame,
            actor == target,
            draw,
        )
        .map(|launch| (cell, vec![launch]))
        .map_err(|_| CastRejection::InvalidState);
    }
    let frame_actor = if spec.shape == bace_magic::ProjectileShape::Strike {
        target
            .filter(|id| *id != actor)
            .ok_or(CastRejection::InvalidTarget)?
    } else {
        actor
    };
    let (cell, _) = world
        .actor_state(frame_actor)
        .map_err(|_| CastRejection::MissingActor)?;
    let source = actor_frame(world, actor, cell)?;
    let target_id = target.unwrap_or(actor);
    let target_frame = actor_frame(world, target_id, cell)?;
    let mut perturbations = Vec::new();
    if spec.perturbation != Vec3::ZERO {
        let mut stream = random
            .fork(b"origins", 0)
            .map_err(|_| CastRejection::InvalidState)?;
        for _ in 0..spec.count {
            perturbations.push(Vec3::new(
                unit(&mut stream)? * 2.0 - 1.0,
                unit(&mut stream)? * 2.0 - 1.0,
                unit(&mut stream)? * 2.0 - 1.0,
            ));
        }
    }
    let launches = if spec.shape == bace_magic::ProjectileShape::Strike {
        bace_magic::ace_strike_projectiles(spec, source, target_frame, &perturbations)
    } else {
        bace_magic::gdle_projectiles(
            spec,
            source,
            target_frame,
            target_id == actor,
            &perturbations,
        )
    };
    launches
        .map(|launches| (cell, launches))
        .map_err(|_| CastRejection::InvalidState)
}
