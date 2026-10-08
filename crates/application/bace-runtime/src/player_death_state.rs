//! Existing ACE scalar properties carry death state; no evolving DTO is persisted.
use bace_simulation::PlayerDeathState;
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
pub fn restore_player_death_state(
    saved: &PlayerSaveV6,
) -> Result<PlayerDeathState, SaveCodecError> {
    let p = &saved.player.entity.state.properties;
    let int = |id, default| {
        p.ints
            .iter()
            .find(|v| v.id == id)
            .map_or(default, |v| v.value)
    };
    let float = |id| p.floats.iter().find(|v| v.id == id).map(|v| v.value);
    let state = PlayerDeathState {
        num_deaths: u32::try_from(int(43, 0))
            .map_err(|_| SaveCodecError::Invalid("death count"))?,
        death_level: u32::try_from(int(139, int(25, 1)))
            .map_err(|_| SaveCodecError::Invalid("death level"))?,
        vitae_pool: int(129, 0),
        olthoi_loot_timestamp: p.ints.iter().find(|v| v.id == 347).map(|v| v.value),
        pk_status: u32::try_from(int(134, 2)).map_err(|_| SaveCodecError::Invalid("PK status"))?,
        pk_respite_elapsed: float(50),
        protection_elapsed: if p.bools.iter().any(|v| v.id == 30 && v.value) {
            Some(float(126).unwrap_or(0.))
        } else {
            None
        },
        last_outside_death: p.positions.iter().find(|p| p.id == 14).map(|p| {
            bace_interactions::PortalPosition {
                cell: p.value.obj_cell_id,
                origin: [p.value.position_x, p.value.position_y, p.value.position_z],
                rotation: [
                    p.value.rotation_w,
                    p.value.rotation_x,
                    p.value.rotation_y,
                    p.value.rotation_z,
                ],
            }
        }),
    };
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("death metadata"))?;
    Ok(state)
}
pub fn freeze_player_death_state(
    saved: &mut PlayerSaveV6,
    state: &PlayerDeathState,
) -> Result<(), SaveCodecError> {
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("death metadata"))?;
    if restore_player_death_state(saved)? == *state {
        return Ok(());
    }
    let p = &mut saved.player.entity.state.properties;
    p.ints.retain(|p| p.id != 347);
    if let Some(timestamp) = state.olthoi_loot_timestamp {
        crate::game_inventory::set(&mut p.ints, 347, timestamp);
    }
    p.positions.retain(|p| p.id != 14);
    if let Some(position) = state.last_outside_death {
        crate::game_inventory::set(
            &mut p.positions,
            14,
            bace_content::Position {
                obj_cell_id: position.cell,
                position_x: position.origin[0],
                position_y: position.origin[1],
                position_z: position.origin[2],
                rotation_w: position.rotation[0],
                rotation_x: position.rotation[1],
                rotation_y: position.rotation[2],
                rotation_z: position.rotation[3],
            },
        );
    }
    for (id, value) in [
        (43, state.num_deaths as i32),
        (139, state.death_level as i32),
        (129, state.vitae_pool),
        (134, state.pk_status as i32),
    ] {
        // Preserve absent default fields on unchanged legacy/new saves.
        let default = match id {
            139 => p.ints.iter().find(|v| v.id == 25).map_or(1, |v| v.value),
            134 => 2,
            _ => 0,
        };
        if value != default || p.ints.iter().any(|v| v.id == id) {
            crate::game_inventory::set(&mut p.ints, id, value);
        }
    }
    for (id, value) in [
        (50, state.pk_respite_elapsed),
        (126, state.protection_elapsed),
    ] {
        p.floats.retain(|v| v.id != id);
        if let Some(value) = value {
            crate::game_inventory::set(&mut p.floats, id, value);
        }
    }
    p.bools.retain(|v| v.id != 30);
    if state.protection_elapsed.is_some() {
        crate::game_inventory::set(&mut p.bools, 30, true);
    }
    Ok(())
}
