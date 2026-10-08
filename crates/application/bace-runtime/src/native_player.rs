//! Frozen/native NPC service projections share the player's aggregate revision.
use crate::game_inventory::set;
use bace_character::CharacterServiceState;
use bace_gameplay_api::NpcDestination;
use bace_storage_codec::{ContractSaveV1, PlayerSaveV6, SaveCodecError};
pub fn restore_services(saved: &PlayerSaveV6) -> Result<CharacterServiceState, SaveCodecError> {
    let props = &saved.player.entity.state.properties;
    let int = |id, default| {
        props
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let total = props
        .int64s
        .iter()
        .find(|p| p.id == 1)
        .map_or(0, |p| p.value);
    let mut titles = saved.player.metadata.titles.clone();
    titles.sort_unstable();
    let sanctuary = props
        .positions
        .iter()
        .find(|p| p.id == 4)
        .map(|p| NpcDestination {
            cell: Some(bace_types::CellId(p.value.obj_cell_id)),
            position: bace_geometry::Vec3::new(
                p.value.position_x,
                p.value.position_y,
                p.value.position_z,
            ),
            rotation: [
                p.value.rotation_w,
                p.value.rotation_x,
                p.value.rotation_y,
                p.value.rotation_z,
            ],
            relative: false,
        });
    let state = CharacterServiceState {
        level: u32::try_from(int(25, 1)).map_err(|_| SaveCodecError::Invalid("player level"))?,
        total_experience: u64::try_from(total).map_err(|_| SaveCodecError::Invalid("total XP"))?,
        total_skill_credits: props.ints.iter().find(|p| p.id == 23).map(|p| p.value),
        titles,
        enlightenment: u32::try_from(int(390, 0))
            .map_err(|_| SaveCodecError::Invalid("enlightenment"))?,
        sanctuary,
    };
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("NPC character service state"))?;
    Ok(state)
}
pub fn restore_contracts(
    saved: &PlayerSaveV6,
) -> Result<bace_quests::ContractRegistry, SaveCodecError> {
    bace_quests::ContractRegistry::restore(
        saved
            .contracts
            .iter()
            .map(|c| bace_quests::ContractState {
                id: c.id,
                display: c.display,
            })
            .collect(),
    )
    .map_err(|_| SaveCodecError::Invalid("contract registry"))
}
pub fn freeze_services(
    saved: &mut PlayerSaveV6,
    state: &CharacterServiceState,
    contracts: &bace_quests::ContractRegistry,
) -> Result<(), SaveCodecError> {
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("NPC character service state"))?;
    let previous = restore_services(saved)?;
    let level = i32::try_from(state.level).map_err(|_| SaveCodecError::Invalid("player level"))?;
    let total_experience =
        i64::try_from(state.total_experience).map_err(|_| SaveCodecError::Invalid("total XP"))?;
    let enlightenment =
        i32::try_from(state.enlightenment).map_err(|_| SaveCodecError::Invalid("enlightenment"))?;
    let props = &mut saved.player.entity.state.properties;
    if state.total_skill_credits != previous.total_skill_credits {
        match state.total_skill_credits {
            Some(value) => set(&mut props.ints, 23, value),
            None => props.ints.retain(|property| property.id != 23),
        }
    }
    if state.level != previous.level {
        set(&mut props.ints, 25, level);
    }
    if state.total_experience != previous.total_experience {
        set(&mut props.int64s, 1, total_experience);
    }
    if state.enlightenment != previous.enlightenment {
        set(&mut props.ints, 390, enlightenment);
    }
    if state.sanctuary != previous.sanctuary {
        if let Some(destination) = &state.sanctuary {
            if destination.relative {
                return Err(SaveCodecError::Invalid("unresolved sanctuary position"));
            }
            let cell = destination
                .cell
                .ok_or(SaveCodecError::Invalid("sanctuary cell"))?;
            set(
                &mut props.positions,
                4,
                bace_content::Position {
                    obj_cell_id: cell.0,
                    position_x: destination.position.x,
                    position_y: destination.position.y,
                    position_z: destination.position.z,
                    rotation_w: destination.rotation[0],
                    rotation_x: destination.rotation[1],
                    rotation_y: destination.rotation[2],
                    rotation_z: destination.rotation[3],
                },
            );
        } else {
            props.positions.retain(|p| p.id != 4);
        }
    }
    if state.titles != previous.titles {
        // ACE GetTitles returns registry order and AddTitleToRegistry appends.
        // The owner uses a sorted membership projection; do not let that reorder
        // the frozen title book or its later CharacterTitle packet.
        saved
            .player
            .metadata
            .titles
            .retain(|id| state.titles.binary_search(id).is_ok());
        saved.player.metadata.titles.extend(
            state
                .titles
                .iter()
                .copied()
                .filter(|id| previous.titles.binary_search(id).is_err()),
        );
    }
    saved.contracts = contracts
        .entries()
        .iter()
        .map(|c| ContractSaveV1 {
            id: c.id,
            display: c.display,
        })
        .collect();
    Ok(())
}
