//! Bounded cold corpse/source preparation. All positions originate from an
//! operation-scoped accepted snapshot; the simulation rechecks them on adoption.
mod corpse;
mod no_corpse;
mod olthoi;
use crate::{game_inventory::FrozenInventoryItem, region_activation::VerifiedRegionAssets};
use bace_content::WeenieV1;
use bace_inventory::ItemPlace;
use bace_storage_codec::{PackGeneration, PackKey, PackLookup, PlayerSaveV6};
use bace_types::{CellId, EntityId};
pub use corpse::{CorpseAppearanceInput, freeze_corpse_metadata, prepare_corpse_source};
pub use no_corpse::{
    NoCorpseLootInput, PlayerNoCorpsePreparationInput, PreparedNoCorpseLoot, prepare_no_corpse_loot,
};
pub use olthoi::{OlthoiLootInput, PreparedOlthoiLoot, prepare_olthoi_loot};
use std::collections::BTreeMap;

pub struct PlayerDeathPreparationInput<'a> {
    pub operation: u64,
    pub player: &'a PlayerSaveV6,
    pub snapshot: &'a bace_simulation::PlayerReadSnapshot,
    pub items: &'a [FrozenInventoryItem],
    pub generation: &'a PackGeneration,
    pub geometry: &'a bace_physics::GeometryRegion,
    /// Fresh persistent-sequence allocations, retained across cold retries.
    pub identities: &'a [EntityId],
    pub killer: Option<(EntityId, &'a str)>,
    pub instantiation: Option<bace_interactions::PortalPosition>,
    pub olthoi: Option<&'a PreparedOlthoiLoot>,
}
pub struct PreparedPlayerDeathCold {
    pub prepared: bace_simulation::PreparedPlayerDeath,
    pub fresh_items: Vec<FrozenInventoryItem>,
    pub positions: BTreeMap<u32, bace_content::Position>,
    pub appearance: crate::player_entry::PreparedEntryAppearanceAssets,
    pub corpse_visibility: crate::visibility_assets::PreparedVisibilityObject,
}
fn template(generation: &PackGeneration, id: u32) -> Result<WeenieV1, String> {
    let PackLookup::Record(row) = generation
        .lookup(PackKey {
            namespace: 1,
            id: u64::from(id),
        })
        .map_err(|e| e.to_string())?
    else {
        return Err(format!("missing death template {id}"));
    };
    if row.bytes().len() > 4 * 1024 * 1024 {
        return Err("death template byte bound".into());
    }
    bace_content_tools::decode(row.bytes()).map_err(|e| e.to_string())
}
fn int(source: &WeenieV1, id: u32) -> Option<i32> {
    source
        .properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
}
/// Allocate only one corpse plus possible split instances and source coin
/// stacks. Selection itself remains on the simulation's single RNG owner.
pub fn death_identity_count(
    items: &[FrozenInventoryItem],
    generation: &PackGeneration,
) -> Result<usize, String> {
    if items.len() > 1023 {
        return Err("death inventory capacity".into());
    }
    let splits = items
        .iter()
        .filter(|i| i.entity.state.weenie_id != 273 && int(&i.entity.state, 12).unwrap_or(1) > 1)
        .count();
    let coins = items
        .iter()
        .filter(|i| i.entity.state.weenie_id == 273)
        .try_fold(0u32, |sum, i| {
            sum.checked_add(u32::try_from(int(&i.entity.state, 12).unwrap_or(1)).ok()?)
        })
        .ok_or("death coin overflow")?;
    let maximum = if coins == 0 {
        1
    } else {
        u32::try_from(int(&template(generation, 273)?, 11).unwrap_or(1))
            .map_err(|_| "invalid coin stack maximum")?
    };
    if maximum == 0 {
        return Err("zero coin stack maximum".into());
    }
    let count = 1usize
        .checked_add(splits)
        .and_then(|v| v.checked_add((coins / 2).div_ceil(maximum) as usize))
        .ok_or("death identity overflow")?;
    if count > 1024 {
        return Err("death fresh identity capacity".into());
    }
    Ok(count)
}
impl VerifiedRegionAssets {
    pub fn prepare_player_death(
        &mut self,
        input: PlayerDeathPreparationInput<'_>,
    ) -> Result<PreparedPlayerDeathCold, String> {
        let actor = input.snapshot.binding().actor;
        if input.operation == 0
            || input.snapshot.operation()
                != Some((
                    bace_simulation::PlayerSnapshotOperation::PlayerDeath(input.operation),
                    input.player.player.entity.mutation_revision,
                ))
            || input.player.player.entity.object_id != actor.0
            || input.items.len() != input.snapshot.items().len()
            || input.identities.len()
                != match input.olthoi {
                    Some(v) => 1 + v.sources.len(),
                    None => death_identity_count(input.items, input.generation)?,
                }
        {
            return Err("death preparation identity/snapshot mismatch".into());
        }
        let ids: std::collections::BTreeSet<_> = input.identities.iter().copied().collect();
        if ids.len() != input.identities.len()
            || ids
                .iter()
                .any(|id| !(0x80000000..=0xfffffffe).contains(&id.0))
        {
            return Err("death allocated identities invalid".into());
        }
        let source = &input.player.player.entity.state;
        let avatar = self.prepare_avatar_dat(source)?;
        let sources: Vec<_> = std::iter::once(source)
            .chain(input.items.iter().map(|i| &i.entity.state))
            .collect();
        let appearance = self.prepare_entry_appearance(&sources)?;
        let equipment:Vec<_>=input.items.iter().filter(|i|matches!(i.placement,Some(bace_storage_codec::ItemPlacementV2::Contained {equipped,..}) if equipped!=0)).map(|i|&i.entity.state).collect();
        let model = crate::player_entry::prepare_player_model(
            source,
            &equipment,
            crate::player_entry::PlayerAppearanceOptions {
                show_helm: input.player.player.metadata.options2 & 0x100000 != 0,
                show_cloak: input.player.player.metadata.options2 & 0x00800000 != 0,
                default_hair_texture: input.player.player.metadata.default_hair_texture,
                hair_texture: input.player.player.metadata.hair_texture,
            },
            &appearance.borrowed(avatar.character.char_gen()),
        )?;
        let PackLookup::Record(classes) = input
            .generation
            .lookup(PackKey {
                namespace: 49,
                id: 1,
            })
            .map_err(|e| e.to_string())?
        else {
            return Err("death class index missing".into());
        };
        let classes = bace_content_tools::decode_template_classes(classes.bytes())?;
        let corpse_id = classes
            .entries
            .iter()
            .find(|c| c.class_name == "corpse")
            .ok_or("accepted corpse class missing")?
            .template;
        let corpse_template = template(input.generation, corpse_id)?;
        let world = input.snapshot.world();
        let half = world.heading * 0.5;
        let position = bace_content::Position {
            obj_cell_id: world.cell.0,
            position_x: world.position.x,
            position_y: world.position.y,
            position_z: world.position.z,
            rotation_w: half.cos(),
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: half.sin(),
        };
        let corpse_source = prepare_corpse_source(CorpseAppearanceInput {
            template: &corpse_template,
            player: source,
            actor,
            position: position.clone(),
            model: &model.model,
            killer: input.killer,
        })?;
        let corpse = input.identities[0];
        let corpse_visibility = self
            .prepare_visibility_sources(vec![crate::visibility_assets::VisibilitySource {
                entity: corpse,
                incarnation: input.operation,
                revision: 1,
                source: &corpse_source,
                equipment: vec![],
                missile_combat: false,
            }])?
            .pop()
            .ok_or("death corpse visibility absent")?;
        let shape = self.prepare_world_item_shape(&corpse_source)?;
        let body = bace_physics::Body::spawn_geometry(
            input.geometry,
            bace_physics::GeometrySpawn {
                cell: world.cell.0,
                position: world.position,
                shape,
                capabilities: bace_motion::Capabilities {
                    speed: 0.,
                    jump_impulse: 0.,
                },
                heading: world.heading,
                maximum_turn_rate: 0.,
            },
        )
        .map_err(|e| e.to_string())?;
        let (corpse_item, corpse_container) = crate::generator_items::prepare_inventory_item(
            &corpse_source,
            corpse,
            0,
            ItemPlace::World,
        )?;
        let mut fresh_items = vec![FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            entity: bace_storage_codec::EntitySaveV1 {
                object_id: corpse.0,
                template_revision: input.generation.revision(),
                mutation_revision: 0,
                state: corpse_source,
            },
            placement: None,
            persisted_version: 0,
            enchantments: vec![],
        }];
        let mut fresh_stacks = Vec::new();
        let mut possessions = Vec::new();
        let mut equipped_health = Vec::new();
        let mut next = 1;
        for item in input.items {
            let live = input
                .snapshot
                .items()
                .iter()
                .find(|i| i.id.0 == item.entity.object_id)
                .ok_or("death item snapshot missing")?;
            let state = &item.entity.state;
            if live.template != state.weenie_id || live.revision != item.entity.mutation_revision {
                return Err("death item snapshot stale".into());
            }
            let wielded = matches!(live.place,ItemPlace::Contained{equipped,..} if equipped!=0);
            possessions.push(bace_interactions::DeathPossession {
                id: live.id,
                template: live.template,
                item_type: int(state, 1).unwrap_or(0) as u32,
                value: int(state, 19).unwrap_or(0),
                stack: live.stack,
                wielded,
                bonded: int(state, 33).unwrap_or(0),
            });
            if wielded {
                equipped_health.push((
                    live.id,
                    u32::try_from(int(state, 379).unwrap_or(0))
                        .map_err(|_| "negative gear health")?,
                ));
            }
            if input.olthoi.is_none() && live.template != 273 && live.stack > 1 {
                let source = template(input.generation, live.template)?;
                let split = crate::stack_factory::prepare_split_stack(
                    &source,
                    input.generation.revision(),
                    input.identities[next],
                    1,
                    ItemPlace::World,
                    false,
                )?;
                next += 1;
                fresh_stacks.push((live.id, split.item));
                fresh_items.push(split.frozen);
            }
        }
        let mut coin_stacks = Vec::new();
        if input.olthoi.is_none() && next < input.identities.len() {
            let coin = template(input.generation, 273)?;
            for id in &input.identities[next..] {
                let stack = crate::stack_factory::prepare_split_stack(
                    &coin,
                    input.generation.revision(),
                    *id,
                    1,
                    ItemPlace::World,
                    false,
                )?;
                coin_stacks.push(stack.item);
                fresh_items.push(stack.frozen);
            }
        }
        let olthoi = if let Some(loot) = input.olthoi {
            let mut generated: Vec<bace_inventory::InventoryItem> =
                Vec::with_capacity(loot.sources.len());
            let mut containers = Vec::new();
            let mut root_slots = [0u32; 2];
            for (index, (node, id)) in loot
                .sources
                .iter()
                .zip(input.identities.iter().skip(1))
                .enumerate()
            {
                let source = &node.source;
                if crate::generator_preparation::is_creature_template(source.weenie_type) {
                    return Err("Olthoi contained creature needs construction owner".into());
                }
                let pack_slot = source
                    .properties
                    .bools
                    .iter()
                    .any(|p| p.id == 81 && p.value);
                let (parent, slot) = if let Some(parent) = node.parent_index {
                    if parent >= index
                        || !generated[parent].is_container
                        || node.inventory_slot == u32::MAX
                    {
                        return Err("Olthoi contained source parent/slot invalid".into());
                    }
                    (input.identities[parent + 1], node.inventory_slot)
                } else {
                    let lane = usize::from(pack_slot);
                    let slot = root_slots[lane];
                    root_slots[lane] = slot.checked_add(1).ok_or("Olthoi root slot overflow")?;
                    (corpse, slot)
                };
                let mut source = source.clone();
                if let Some(parent) = node.generator_parent_index {
                    if parent >= index || !generated[parent].is_container {
                        return Err("Olthoi generated parent order invalid".into());
                    }
                    crate::game_inventory::set(
                        &mut source.properties.instance_ids,
                        6,
                        input.identities[parent + 1].0,
                    );
                }
                let (item, container) = crate::generator_items::prepare_inventory_item(
                    &source,
                    *id,
                    0,
                    ItemPlace::Contained {
                        container: parent,
                        slot,
                        equipped: 0,
                    },
                )?;
                containers.extend(container);
                generated.push(item);
                fresh_items.push(FrozenInventoryItem {
                    corpse: None,
                    construction: None,
                    source_destination: node.source_destination,
                    entity: bace_storage_codec::EntitySaveV1 {
                        object_id: id.0,
                        template_revision: input.generation.revision(),
                        mutation_revision: 0,
                        state: source,
                    },
                    placement: None,
                    persisted_version: 0,
                    enchantments: vec![],
                });
            }
            Some(bace_simulation::PreparedOlthoiDeath {
                kind: loot.kind,
                had_vitae: loot.had_vitae,
                before_timestamp: loot.before_timestamp,
                after_timestamp: loot.after_timestamp,
                items: generated,
                containers,
            })
        } else {
            None
        };
        let formula = |f: bace_dat::SkillFormula| bace_character::VitalFormula {
            enabled: f.x != 0,
            divisor: f.z,
            attribute1: f.attribute1,
            attribute2: f.attribute2,
        };
        let animation_seconds = self.prepare_player_death_animation(source)?;
        Ok(PreparedPlayerDeathCold {
            prepared: bace_simulation::PreparedPlayerDeath {
                operation: input.operation,
                actor,
                corpse: bace_entity::Actor {
                    id: corpse,
                    cell: CellId(world.cell.0),
                    body,
                },
                corpse_item,
                corpse_container: corpse_container.ok_or("corpse container source missing")?,
                possessions,
                fresh_stacks,
                coin_stacks,
                animation_seconds,
                vital_formulas: [
                    formula(avatar.vitals.health),
                    formula(avatar.vitals.stamina),
                    formula(avatar.vitals.mana),
                ],
                equipped_health,
                instantiation: input.instantiation,
                olthoi,
            },
            fresh_items,
            positions: BTreeMap::from([(corpse.0, position)]),
            appearance,
            corpse_visibility,
        })
    }
}
