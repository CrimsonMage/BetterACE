//! Pure portal/link preparation from frozen native aggregates. Saved anchor values
//! are copied; mutating a recalled frame cannot alias Sanctuary/LinkedLifestone.
mod locations;
mod template;
use bace_content::{Position, Property, WeenieV1};
use bace_interactions::{
    PortalAnchor, PortalError, PortalLinkMutation, PortalLinks, PortalPosition,
};
pub use locations::prepare_recall_locations;
pub use template::prepare_portal_template;
fn position(p: &Position) -> PortalPosition {
    PortalPosition {
        cell: p.obj_cell_id,
        origin: [p.position_x, p.position_y, p.position_z],
        rotation: [p.rotation_w, p.rotation_x, p.rotation_y, p.rotation_z],
    }
}
fn frozen(p: PortalPosition) -> Position {
    Position {
        obj_cell_id: p.cell,
        position_x: p.origin[0],
        position_y: p.origin[1],
        position_z: p.origin[2],
        rotation_w: p.rotation[0],
        rotation_x: p.rotation[1],
        rotation_y: p.rotation[2],
        rotation_z: p.rotation[3],
    }
}
pub fn prepare_portal_links(player: &WeenieV1, revision: u64) -> Result<PortalLinks, PortalError> {
    let positions: Vec<_> = player
        .properties
        .positions
        .iter()
        .filter(|p| matches!(p.id, 4 | 8 | 9 | 15 | 16))
        .map(|p| (p.id as u16, position(&p.value)))
        .collect();
    let templates: Vec<_> = player
        .properties
        .data_ids
        .iter()
        .filter(|p| matches!(p.id, 31 | 47 | 48))
        .map(|p| (p.id as u16, p.value))
        .collect();
    let mut links = PortalLinks::new(revision, &positions, &templates)?;
    links.restore_summoned_options(
        player
            .properties
            .bools
            .iter()
            .find(|p| p.id == 9001)
            .map(|p| p.value),
        player
            .properties
            .bools
            .iter()
            .find(|p| p.id == 9002)
            .map(|p| p.value),
    );
    Ok(links)
}
pub fn freeze_portal_link(
    player: &WeenieV1,
    change: &PortalLinkMutation,
) -> Result<WeenieV1, PortalError> {
    let mut links = prepare_portal_links(player, change.before_revision)?;
    links.adopt(change)?;
    freeze_portal_links(player, &links)
}

pub fn prepare_portal_anchor(
    entity: u32,
    template: &WeenieV1,
    accepted_position: PortalPosition,
) -> Result<PortalAnchor, PortalError> {
    prepare_portal_template(template)?.instantiate(entity, accepted_position)
}

/// Merge the complete current link owner into routine saves without touching
/// Location(slot1), combat vitals or unrelated sparse properties.
pub fn freeze_portal_links(
    player: &WeenieV1,
    links: &PortalLinks,
) -> Result<WeenieV1, PortalError> {
    let mut after = player.clone();
    for slot in [4, 8, 9, 15, 16] {
        let value = links.position(slot);
        if let Some(value) = value {
            value.validate()?;
        }
        merge_property(
            &mut after.properties.positions,
            u32::from(slot),
            value.map(frozen),
        );
    }
    for slot in [31, 47, 48] {
        merge_property(
            &mut after.properties.data_ids,
            u32::from(slot),
            links.template(slot),
        );
    }
    for (slot, id) in [(31, 9001), (48, 9002)] {
        merge_property(&mut after.properties.bools, id, links.summoned_flag(slot));
    }
    Ok(after)
}
/// Preserve accepted sparse-vector order. Existing fields update in place;
/// newly introduced fields append and only an explicit absence removes a field.
fn merge_property<T>(properties: &mut Vec<Property<T>>, id: u32, value: Option<T>) {
    if let Some(value) = value {
        if let Some(property) = properties.iter_mut().find(|p| p.id == id) {
            property.value = value;
        } else {
            properties.push(Property { id, value });
        }
    } else {
        properties.retain(|p| p.id != id);
    }
}
