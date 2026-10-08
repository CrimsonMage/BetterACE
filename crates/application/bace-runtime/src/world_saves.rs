//! Accepted physics/vitals overwrite their frozen fields; client reports never do.
use bace_simulation::PlayerWorldSnapshot;
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
pub fn freeze_world(
    saved: &mut PlayerSaveV6,
    world: PlayerWorldSnapshot,
) -> Result<(), SaveCodecError> {
    if world.cell.0 == 0
        || !world.position.x.is_finite()
        || !world.position.y.is_finite()
        || !world.position.z.is_finite()
        || !world.heading.is_finite()
    {
        return Err(SaveCodecError::Invalid("accepted player world snapshot"));
    }
    let mut next = saved.clone();
    let props = &mut next.player.entity.state.properties;
    let half = world.heading * 0.5;
    crate::game_inventory::set(
        &mut props.positions,
        1,
        bace_content::Position {
            obj_cell_id: world.cell.0,
            position_x: world.position.x,
            position_y: world.position.y,
            position_z: world.position.z,
            rotation_w: half.cos(),
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: half.sin(),
        },
    );
    for (key, pool) in [1, 3, 5].into_iter().zip(world.vitals) {
        if let Some(pool) = pool {
            if pool.current > pool.maximum {
                return Err(SaveCodecError::Invalid("accepted vital pool"));
            }
            let value = props
                .secondary_attributes
                .iter_mut()
                .find(|p| p.id == key)
                .ok_or(SaveCodecError::Invalid("missing saved vital definition"))?;
            value.value.current_level = pool.current;
        }
    }
    next.validate()?;
    *saved = next;
    Ok(())
}
