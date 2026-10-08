//! Preserve constructor evidence beside the full root/child item snapshots.
use bace_gameplay_api::GeneratorSpawnIntent;
use bace_loot::PreparedCreatureEquipment;
use bace_simulation::PreparedNpcLoadout;
use bace_storage_codec::{
    FrozenConstructedChildV1, FrozenCreatureConstructionV1, FrozenGeneratorConstructionOriginV1,
};
use bace_types::EntityId;

pub(super) fn freeze(
    actor: EntityId,
    weenie_type: u32,
    intent: &GeneratorSpawnIntent,
    gear: &[(EntityId, PreparedCreatureEquipment)],
    loadout: &PreparedNpcLoadout,
) -> Result<FrozenCreatureConstructionV1, String> {
    let mut equipment: Vec<_> = gear
        .iter()
        .filter_map(|(id, item)| item.equip_order.map(|order| (order, id.0)))
        .collect();
    equipment.sort_by_key(|&(order, _)| order);
    if equipment.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("duplicate creature equipment construction order".into());
    }
    let death_roster = loadout
        .death_ids
        .iter()
        .zip(&loadout.death_parents)
        .map(|(&entity, &parent)| {
            Ok(FrozenConstructedChildV1 {
                entity: entity.0,
                parent: parent
                    .map(|index| {
                        loadout
                            .death_ids
                            .get(index)
                            .map(|id| id.0)
                            .ok_or("invalid creature death parent".to_string())
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<_, String>>()?;
    let value = FrozenCreatureConstructionV1 {
        weenie_type,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: intent.key.generator.entity.0,
            incarnation: intent.key.generator.incarnation,
            content_revision: intent.key.generator.content_revision,
            profile: intent.key.profile_id,
            occurrence: intent.key.occurrence,
            random_identity: intent.random_identity,
            random_key_version: intent.random_key_version,
        },
        equipment_order: equipment.into_iter().map(|(_, id)| id).collect(),
        death_roster,
    };
    value
        .validate(actor.0, weenie_type)
        .map_err(|e| e.to_string())?;
    Ok(value)
}
