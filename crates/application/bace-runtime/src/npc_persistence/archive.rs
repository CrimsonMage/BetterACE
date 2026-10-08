//! Explicit detached-source DTO conversion. Live EntityProperties are never
//! serialized, and descriptive source coordinates never instantiate physics.
use super::effects::{
    freeze_property_family, freeze_property_value, thaw_property_family, thaw_property_value,
};
use bace_storage_codec::{NpcArchivedPropertyV3, NpcSourceArchiveV3, SaveCodecError};
pub(super) fn freeze(
    value: bace_simulation::NpcSourceArchive,
) -> Result<NpcSourceArchiveV3, SaveCodecError> {
    value
        .validate()
        .map_err(|_| SaveCodecError::Invalid("NPC source archive"))?;
    let (property_revision, properties) = value.properties.snapshot();
    let archive = NpcSourceArchiveV3 {
        source: value.source.0,
        player: value.facts.player,
        creature: value.facts.creature,
        cell: value.cell.0,
        position: [value.position.x, value.position.y, value.position.z],
        heading: value.heading,
        property_revision,
        properties: properties
            .into_iter()
            .map(|(family, stat, value)| NpcArchivedPropertyV3 {
                family: freeze_property_family(family),
                stat,
                value: freeze_property_value(value),
            })
            .collect(),
    };
    archive.validate()?;
    Ok(archive)
}
pub(super) fn thaw(
    value: NpcSourceArchiveV3,
) -> Result<bace_simulation::NpcSourceArchive, SaveCodecError> {
    value.validate()?;
    let properties = bace_entity::EntityProperties::restore_snapshot(
        value.property_revision,
        value
            .properties
            .into_iter()
            .map(|p| {
                (
                    thaw_property_family(p.family),
                    p.stat,
                    thaw_property_value(p.value),
                )
            })
            .collect(),
    )
    .map_err(|_| SaveCodecError::Invalid("NPC property archive"))?;
    Ok(bace_simulation::NpcSourceArchive {
        source: bace_types::EntityId(value.source),
        facts: bace_emotes::NpcActorFacts {
            player: value.player,
            creature: value.creature,
        },
        cell: bace_types::CellId(value.cell),
        position: bace_geometry::Vec3::new(value.position[0], value.position[1], value.position[2]),
        heading: value.heading,
        properties,
    })
}
