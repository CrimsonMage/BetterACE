//! Source-ordered Bool29 treasure selection before allocating durable world IDs.
//! This is immutable cold preparation; the player death ticket separately owns
//! inventory removal, world placement, and receipt publication.
use super::*;
use bace_content::{TreasureDeathRowV1, WorldRecordV1};
use bace_loot::PreparedContainerItem;
use std::sync::Arc;

pub struct PlayerNoCorpsePreparationInput<'a> {
    pub operation: u64,
    pub player: &'a PlayerSaveV6,
    pub snapshot: &'a bace_simulation::PlayerReadSnapshot,
    pub items: &'a [FrozenInventoryItem],
    pub generation: &'a PackGeneration,
    pub geometry: &'a bace_physics::GeometryRegion,
    pub identities: &'a [EntityId],
    pub loot: &'a PreparedNoCorpseLoot,
    pub instantiation: Option<bace_interactions::PortalPosition>,
}

pub struct PreparedPlayerNoCorpseCold {
    pub prepared: bace_simulation::PreparedPlayerNoCorpse,
    pub fresh_items: Vec<FrozenInventoryItem>,
    pub positions: BTreeMap<u32, bace_content::Position>,
    pub appearance: crate::player_entry::PreparedEntryAppearanceAssets,
    pub world_visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
}

impl VerifiedRegionAssets {
    pub fn prepare_player_no_corpse_death(
        &mut self,
        input: PlayerNoCorpsePreparationInput<'_>,
    ) -> Result<PreparedPlayerNoCorpseCold, String> {
        let actor = input.snapshot.binding().actor;
        if input.operation == 0
            || input.snapshot.operation()
                != Some((
                    bace_simulation::PlayerSnapshotOperation::PlayerDeath(input.operation),
                    input.player.player.entity.mutation_revision,
                ))
            || input.player.player.entity.object_id != actor.0
            || input.items.len() != input.snapshot.items().len()
            || input.identities.len() != input.loot.sources.len()
            || input.identities.len() + input.loot.existing.len() > 1024
        {
            return Err("NoCorpse preparation identity/snapshot mismatch".into());
        }
        let ids: std::collections::BTreeSet<_> = input.identities.iter().copied().collect();
        if ids.len() != input.identities.len()
            || ids
                .iter()
                .any(|id| !(0x80000000..=0xfffffffe).contains(&id.0))
            || input.loot.existing.iter().any(|id| ids.contains(id))
        {
            return Err("NoCorpse allocated identities invalid".into());
        }
        let source = &input.player.player.entity.state;
        let avatar = self.prepare_avatar_dat(source)?;
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
        let mut fresh_items = Vec::with_capacity(input.identities.len());
        let mut generated: Vec<bace_inventory::InventoryItem> =
            Vec::with_capacity(input.identities.len());
        let mut containers = Vec::new();
        let mut root_sources: Vec<(EntityId, bace_content::WeenieV1, u64)> = Vec::new();
        let mut root_slots = [0u32; 2];
        for id in &input.loot.existing {
            let baseline = input
                .items
                .iter()
                .find(|item| item.entity.object_id == id.0)
                .ok_or("NoCorpse existing source absent")?;
            let live = input
                .snapshot
                .items()
                .iter()
                .find(|item| item.id == *id)
                .ok_or("NoCorpse existing snapshot absent")?;
            if baseline.entity.mutation_revision != live.revision
                || baseline.entity.state.weenie_id != live.template
                || baseline.persisted_version <= 0
                || !matches!(live.place, ItemPlace::Contained { container, .. } if container == actor)
            {
                return Err("NoCorpse existing source stale".into());
            }
            let mut visible = baseline.entity.state.clone();
            if visible
                .properties
                .strings
                .iter()
                .any(|p| p.id == 33 && !p.value.is_empty())
            {
                crate::game_inventory::set(&mut visible.properties.instance_ids, 6, actor.0);
            }
            let revision = baseline
                .persisted_version
                .checked_add(1)
                .and_then(|value| u64::try_from(value).ok())
                .filter(|value| *value != 0)
                .ok_or("NoCorpse existing visible version overflow")?;
            root_sources.push((*id, visible, revision));
        }
        for (index, (node, id)) in input.loot.sources.iter().zip(input.identities).enumerate() {
            if crate::generator_preparation::is_creature_template(node.source.weenie_type) {
                return Err("NoCorpse creature-valued drop requires construction owner".into());
            }
            let mut state = node.source.clone();
            let place = if let Some(parent) = node.parent_index {
                if parent >= index
                    || !generated[parent].is_container
                    || node.inventory_slot == u32::MAX
                {
                    return Err("NoCorpse child parent/slot invalid".into());
                }
                ItemPlace::Contained {
                    container: input.identities[parent],
                    slot: node.inventory_slot,
                    equipped: 0,
                }
            } else {
                let lane =
                    usize::from(state.properties.bools.iter().any(|p| p.id == 81 && p.value));
                root_slots[lane] = root_slots[lane]
                    .checked_add(1)
                    .ok_or("NoCorpse root slot overflow")?;
                if state
                    .properties
                    .strings
                    .iter()
                    .any(|p| p.id == 33 && !p.value.is_empty())
                {
                    crate::game_inventory::set(&mut state.properties.instance_ids, 6, actor.0);
                }
                root_sources.push((*id, state.clone(), 1));
                ItemPlace::World
            };
            if let Some(parent) = node.generator_parent_index {
                if parent >= index || !generated[parent].is_container {
                    return Err("NoCorpse generated parent invalid".into());
                }
                crate::game_inventory::set(
                    &mut state.properties.instance_ids,
                    6,
                    input.identities[parent].0,
                );
            }
            let (item, container) =
                crate::generator_items::prepare_inventory_item(&state, *id, 0, place)?;
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
                    state,
                },
                placement: None,
                persisted_version: 0,
                enchantments: vec![],
            });
        }
        let mut world_roots = Vec::with_capacity(root_sources.len());
        let mut positions = BTreeMap::new();
        for (id, state, _) in &root_sources {
            let shape = self.prepare_world_item_shape(state)?;
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
            if body.accepted().position() != world.position {
                return Err("NoCorpse copied root pose changed by geometry".into());
            }
            positions.insert(id.0, position.clone());
            world_roots.push(bace_entity::Actor {
                id: *id,
                cell: CellId(world.cell.0),
                body,
            });
        }
        let appearance_sources: Vec<_> = std::iter::once(source)
            .chain(input.items.iter().map(|i| &i.entity.state))
            .chain(fresh_items.iter().map(|i| &i.entity.state))
            .collect();
        let appearance = self.prepare_entry_appearance(&appearance_sources)?;
        let visibility = self.prepare_visibility_sources(
            root_sources
                .iter()
                .map(
                    |(id, state, revision)| crate::visibility_assets::VisibilitySource {
                        entity: *id,
                        incarnation: input.operation,
                        revision: *revision,
                        source: state,
                        equipment: vec![],
                        missile_combat: false,
                    },
                )
                .collect(),
        )?;
        let mut equipped_health = Vec::new();
        for item in input.items {
            let Some(live) = input
                .snapshot
                .items()
                .iter()
                .find(|v| v.id.0 == item.entity.object_id)
            else {
                return Err("NoCorpse item snapshot missing".into());
            };
            if live.revision != item.entity.mutation_revision
                || live.template != item.entity.state.weenie_id
            {
                return Err("NoCorpse item snapshot stale".into());
            }
            if matches!(live.place, ItemPlace::Contained { equipped, .. } if equipped != 0) {
                equipped_health.push((
                    live.id,
                    u32::try_from(int(&item.entity.state, 379).unwrap_or(0))
                        .map_err(|_| "negative gear health")?,
                ));
            }
        }
        let formula = |f: bace_dat::SkillFormula| bace_character::VitalFormula {
            enabled: f.x != 0,
            divisor: f.z,
            attribute1: f.attribute1,
            attribute2: f.attribute2,
        };
        Ok(PreparedPlayerNoCorpseCold {
            prepared: bace_simulation::PreparedPlayerNoCorpse {
                operation: input.operation,
                actor,
                existing: input.loot.existing.clone(),
                fresh_items: generated,
                fresh_containers: containers,
                world_roots,
                accepted_position: position,
                animation_seconds: self.prepare_player_death_animation(source)?,
                vital_formulas: [
                    formula(avatar.vitals.health),
                    formula(avatar.vitals.stamina),
                    formula(avatar.vitals.mana),
                ],
                equipped_health,
                instantiation: input.instantiation,
            },
            fresh_items,
            positions,
            appearance,
            world_visibility: visibility,
        })
    }
}

pub struct NoCorpseLootInput<'a> {
    pub player: &'a PlayerSaveV6,
    pub items: &'a [FrozenInventoryItem],
    pub generation: &'a PackGeneration,
    pub assets: Arc<bace_loot::TreasureAssets>,
    pub aetheria_rate: f32,
    pub root: Arc<bace_random::RandomRoot>,
    pub epoch: u64,
    pub operation: u64,
    pub killer_is_olthoi: bool,
    pub drop_plain_wield: bool,
}

pub struct PreparedNoCorpseLoot {
    /// Original actor-owned roots, in ACE Inventory then Equipped order.
    pub existing: Vec<EntityId>,
    /// Fresh death-table and CreateList roots/descendants in source order.
    pub sources: Vec<PreparedContainerItem>,
}

pub fn prepare_no_corpse_loot(
    input: NoCorpseLootInput<'_>,
) -> Result<PreparedNoCorpseLoot, String> {
    let source = &input.player.player.entity.state;
    let actor = EntityId(input.player.player.entity.object_id);
    if input.epoch == 0
        || input.operation == 0
        || input.items.len() > 1024
        || !source
            .properties
            .bools
            .iter()
            .any(|p| p.id == 29 && p.value)
        || !input.aetheria_rate.is_finite()
    {
        return Err("NoCorpse source bounds".into());
    }
    // Creature_Death.CreateCorpse exits before GenerateTreasure for an Olthoi
    // killer, even when the victim has authored death-table/CreateList rows.
    if input.killer_is_olthoi {
        return Ok(PreparedNoCorpseLoot {
            existing: vec![],
            sources: vec![],
        });
    }
    let mut event = [0; 16];
    event[..8].copy_from_slice(&input.epoch.to_le_bytes());
    event[8..].copy_from_slice(&input.operation.to_le_bytes());
    let mut random = input
        .root
        .event_stream(event, bace_random::Domain::PlayerDeath)
        .and_then(|stream| stream.fork(b"no-corpse", u64::from(actor.0)))
        .map_err(|e| format!("NoCorpse random: {e:?}"))?;
    let mut sources = Vec::new();
    if let Some(treasure_type) = source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 35 && p.value != 0)
        .map(|p| p.value)
    {
        let profile = death_profile(input.generation, treasure_type)?;
        let treasure =
            bace_loot::DeathTreasure::prepare(profile, input.assets.clone(), input.aetheria_rate)
                .map_err(|e| format!("NoCorpse death treasure: {e:?}"))?;
        for root in treasure
            .generate(&mut random)
            .map_err(|e| format!("NoCorpse death treasure roll: {e:?}"))?
        {
            append_tree(
                &mut sources,
                bace_loot::materialize_container_tree(root, &input.assets.templates)
                    .map_err(|e| format!("NoCorpse death tree: {e:?}"))?,
            )?;
        }
    }
    let mut existing = Vec::new();
    for equipped in [false, true] {
        for item in input.items {
            let Some(bace_storage_codec::ItemPlacementV2::Contained {
                container,
                equipped: location,
                ..
            }) = &item.placement
            else {
                continue;
            };
            if *container != actor.0 || (*location != 0) != equipped {
                continue;
            }
            let flags = item
                .source_destination
                .ok_or("NoCorpse legacy item source destination unknown")?;
            let matched = flags & if input.drop_plain_wield { 10 } else { 8 } != 0;
            if matched && int(&item.entity.state, 33) != Some(-2) {
                existing.push(EntityId(item.entity.object_id));
            }
        }
    }
    if existing.len() > 1024 {
        return Err("NoCorpse existing root capacity".into());
    }
    let create = bace_loot::generate_player_no_corpse_create_list(
        source,
        &input.assets.templates,
        &mut random,
    )
    .map_err(|e| format!("NoCorpse CreateList: {e:?}"))?;
    append_tree(
        &mut sources,
        create
            .into_iter()
            .map(|node| PreparedContainerItem {
                source: node.source,
                source_destination: node.source_destination,
                parent_index: node.parent_index,
                generator_parent_index: node.parent_index,
                inventory_slot: node.inventory_slot,
            })
            .collect(),
    )?;
    if existing.len() + sources.len() > 1024 {
        return Err("NoCorpse selected item capacity".into());
    }
    Ok(PreparedNoCorpseLoot { existing, sources })
}

fn append_tree(
    output: &mut Vec<PreparedContainerItem>,
    nodes: Vec<PreparedContainerItem>,
) -> Result<(), String> {
    let offset = output.len();
    if offset.checked_add(nodes.len()).is_none_or(|len| len > 1024) {
        return Err("NoCorpse fresh item capacity".into());
    }
    for (index, mut node) in nodes.into_iter().enumerate() {
        if node.parent_index.is_some_and(|parent| parent >= index) {
            return Err("NoCorpse source parent order".into());
        }
        node.parent_index = node.parent_index.map(|parent| parent + offset);
        node.generator_parent_index = node.generator_parent_index.map(|parent| parent + offset);
        output.push(node);
    }
    Ok(())
}

fn death_profile(
    generation: &PackGeneration,
    treasure_type: u32,
) -> Result<TreasureDeathRowV1, String> {
    let mut cursor = Some(PackKey {
        namespace: 39,
        id: 0,
    });
    let mut count = 0;
    loop {
        let batch = generation.scan(cursor, 256).map_err(|e| e.to_string())?;
        if batch.is_empty() {
            break;
        }
        for (key, value) in batch {
            cursor = Some(key);
            if key.namespace != 39 {
                return Err(format!(
                    "NoCorpse death treasure type {treasure_type} absent"
                ));
            }
            count += 1;
            if count > 65536 {
                return Err("NoCorpse death table capacity".into());
            }
            let PackLookup::Record(record) = value else {
                continue;
            };
            if record.bytes().len() > 4096 {
                return Err("NoCorpse death table row size".into());
            }
            let WorldRecordV1::TreasureDeath(profile) =
                bace_content_tools::decode_world_record(record.bytes())?
            else {
                return Err("NoCorpse death table namespace".into());
            };
            if profile.treasure_type == treasure_type {
                return Ok(profile);
            }
        }
    }
    Err(format!(
        "NoCorpse death treasure type {treasure_type} absent"
    ))
}
