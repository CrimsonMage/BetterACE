//! Live scalar/quest snapshots share the versioned source journal; they never
//! restore a physical body or replace another entity's canonical state.
use super::effects::{
    freeze_property_family, freeze_property_value, thaw_property_family, thaw_property_value,
};
use bace_storage_codec::{
    NpcArchivedPropertyV3, SaveCodecError,
    npc_workflow_v3::{NpcLivePropertiesV3, NpcQuestEntryV3, NpcSourceQuestsV3},
};
pub(super) fn freeze_properties(
    value: bace_entity::EntityProperties,
) -> Result<NpcLivePropertiesV3, SaveCodecError> {
    let (revision, rows) = value.snapshot();
    let value = NpcLivePropertiesV3 {
        revision,
        properties: rows
            .into_iter()
            .map(|(family, stat, value)| NpcArchivedPropertyV3 {
                family: freeze_property_family(family),
                stat,
                value: freeze_property_value(value),
            })
            .collect(),
    };
    value.validate()?;
    Ok(value)
}
pub(super) fn thaw_properties(
    value: NpcLivePropertiesV3,
) -> Result<bace_entity::EntityProperties, SaveCodecError> {
    value.validate()?;
    bace_entity::EntityProperties::restore_snapshot(
        value.revision,
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
    .map_err(|_| SaveCodecError::Invalid("NPC live source properties"))
}
pub(super) fn freeze_quests(
    value: (u64, Vec<(String, bace_quests::QuestProgress)>),
) -> Result<NpcSourceQuestsV3, SaveCodecError> {
    let value = NpcSourceQuestsV3 {
        revision: value.0,
        entries: value
            .1
            .into_iter()
            .map(|(name, p)| NpcQuestEntryV3 {
                name,
                last_completed_seconds: p.last_completed_seconds,
                completions: p.completions,
            })
            .collect(),
    };
    value.validate()?;
    Ok(value)
}
pub(super) fn thaw_quests(
    value: NpcSourceQuestsV3,
) -> Result<(u64, Vec<(String, bace_quests::QuestProgress)>), SaveCodecError> {
    value.validate()?;
    Ok((
        value.revision,
        value
            .entries
            .into_iter()
            .map(|p| {
                (
                    p.name,
                    bace_quests::QuestProgress {
                        last_completed_seconds: p.last_completed_seconds,
                        completions: p.completions,
                    },
                )
            })
            .collect(),
    ))
}

pub(super) fn freeze_location(
    value: bace_simulation::NpcSourceLocation,
) -> Result<bace_storage_codec::npc_workflow_v3::NpcSourceLocationV3, SaveCodecError> {
    value
        .validate()
        .map_err(|_| SaveCodecError::Invalid("NPC source position"))?;
    Ok(bace_storage_codec::npc_workflow_v3::NpcSourceLocationV3 {
        cell: value.cell.0,
        position: [value.position.x, value.position.y, value.position.z],
        heading: value.heading,
        player: value.facts.player,
        creature: value.facts.creature,
    })
}
pub(super) fn thaw_location(
    value: bace_storage_codec::npc_workflow_v3::NpcSourceLocationV3,
) -> Result<bace_simulation::NpcSourceLocation, SaveCodecError> {
    value.validate()?;
    Ok(bace_simulation::NpcSourceLocation {
        cell: bace_types::CellId(value.cell),
        position: bace_geometry::Vec3::new(value.position[0], value.position[1], value.position[2]),
        heading: value.heading,
        facts: bace_emotes::NpcActorFacts {
            player: value.player,
            creature: value.creature,
        },
    })
}

pub(super) fn freeze_inventory(
    value: bace_simulation::NpcSourceInventorySnapshot,
) -> Result<bace_storage_codec::npc_workflow_v3::NpcSourceInventoryV3, SaveCodecError> {
    use bace_storage_codec::npc_workflow_v3::{NpcSourceInventoryItemV3, NpcSourceInventoryV3};
    let snapshot = NpcSourceInventoryV3 {
        source_persisted_version: 0,
        source_mutation_revision: 0,
        origin: value.origin.map(
            |o| bace_storage_codec::npc_workflow_v3::NpcGeneratorOriginV3 {
                generator: o.generator.0,
                incarnation: o.incarnation,
                content_revision: o.content_revision,
                profile: o.profile,
                child_incarnation: o.child_incarnation,
            },
        ),
        ticket: value.ticket,
        source_registry_revision: value.source_registry_revision,
        source_enchantments: value
            .source_enchantments
            .iter()
            .map(crate::enchantment_saves::freeze_enchantment)
            .collect::<Result<_, _>>()
            .map_err(|_| SaveCodecError::Invalid("NPC source registry"))?,
        death_items: value.death_items.into_iter().map(|id| id.0).collect(),
        items: value
            .items
            .into_iter()
            .map(|captured| {
                let bace_inventory::ItemPlace::Contained {
                    container,
                    slot,
                    equipped,
                } = captured.item.place
                else {
                    return Err(SaveCodecError::Invalid("NPC inventory ownership"));
                };
                Ok(NpcSourceInventoryItemV3 {
                    id: captured.item.id.0,
                    revision: captured.item.revision,
                    persisted_version: 0,
                    registry_revision: captured.registry_revision,
                    container: container.0,
                    slot,
                    pack_slot: captured.item.pack_slot,
                    equipped,
                })
            })
            .collect::<Result<_, _>>()?,
    };
    snapshot.validate(value.source.0)?;
    Ok(snapshot)
}
