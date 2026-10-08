//! Cold source materialization; successful output is retained before allocating
//! further identities. All random draws use the immutable occurrence identity.
use super::*;
use crate::region_unload_saves::RegionItemSource;
use bace_content::{Property, WeenieV1};
use bace_gameplay_api::{GeneratorDestination, GeneratorSpawnReceipt, GeneratorSpawnResult};
use bace_inventory::ItemPlace;
use bace_loot::{PreparedContainerItem, PreparedCreatureEquipment};

pub(super) enum Materialized {
    Mixed(Vec<Materialized>),
    Items(Vec<PreparedContainerItem>),
    NestedItems {
        items: Vec<PreparedContainerItem>,
        creatures: Vec<NestedCreatureMaterialization>,
    },
    Creature {
        source: Arc<WeenieV1>,
        gear: Vec<PreparedCreatureEquipment>,
    },
}
pub(super) struct NestedCreatureMaterialization {
    pub root_index: usize,
    pub gear_start: usize,
    pub source: WeenieV1,
    pub gear: Vec<PreparedCreatureEquipment>,
}
impl Materialized {
    pub fn count(&self) -> usize {
        match self {
            Self::Mixed(roots) => roots.iter().map(Self::count).sum(),
            Self::Items(items) => items.len(),
            Self::NestedItems { items, creatures } => {
                items.len()
                    + creatures
                        .iter()
                        .map(|creature| creature.gear.len())
                        .sum::<usize>()
            }
            Self::Creature { gear, .. } => gear.len() + 1,
        }
    }
    pub fn bytes(&self) -> Result<usize, String> {
        if let Self::Mixed(roots) = self {
            return roots.iter().try_fold(0usize, |sum, r| {
                sum.checked_add(r.bytes()?)
                    .filter(|v| *v <= MAX_BYTES)
                    .ok_or_else(|| "generator materialization byte capacity".into())
            });
        }

        let items: Box<dyn Iterator<Item = &WeenieV1> + '_> = match self {
            Self::Mixed(_) => return Err("nested mixed materialization".into()),
            Self::Items(items) => Box::new(items.iter().map(|i| &i.source)),
            Self::NestedItems { items, creatures } => Box::new(
                items.iter().map(|item| &item.source).chain(
                    creatures
                        .iter()
                        .flat_map(|creature| creature.gear.iter().map(|item| &item.source)),
                ),
            ),
            Self::Creature { source, gear } => {
                Box::new(std::iter::once(source.as_ref()).chain(gear.iter().map(|i| &i.source)))
            }
        };
        let mut bytes = 0usize;
        for item in items {
            bytes = bytes
                .checked_add(
                    bace_content_tools::compile_template(item)
                        .map_err(|e| e.to_string())?
                        .len(),
                )
                .ok_or("generator materialization byte overflow")?;
            if bytes > 64 * 1024 * 1024 {
                return Err("generator materialization byte capacity".into());
            }
        }
        Ok(bytes)
    }
}
pub(super) struct Ready {
    pub action: GeneratorAction,
    pub sources: Vec<RegionItemSource>,
}
pub(super) fn materialize(
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    assets: &bace_loot::TreasureAssets,
    random: &bace_random::RandomRoot,
    drop_plain_wield: bool,
) -> Result<Materialized, String> {
    let intent = &request.intent;
    let template = region
        .catalog
        .templates
        .get(&intent.profile.weenie_class_id);
    let roots = crate::generator_items::materialize_generator_items(
        intent,
        template.map(Arc::as_ref),
        region.catalog.treasure.get(&intent.profile.weenie_class_id),
        assets,
        random,
    )?;
    super::mixed::materialize(region, request, roots, random, drop_plain_wield)
}

pub(super) fn bind(
    assets: Result<&mut crate::region_activation::VerifiedRegionAssets, String>,
    table: &mut Option<Arc<bace_dat::SpellTable>>,
    generation: &bace_storage_codec::PackGeneration,
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    raw: &Materialized,
) -> Result<Ready, String> {
    let key = request.intent.key;
    if raw.count() == 0 {
        return Ok(Ready {
            action: GeneratorAction::SpawnReceipt(GeneratorSpawnReceipt {
                key,
                result: GeneratorSpawnResult::Completed {
                    members: vec![],
                    materialized: false,
                    failed_placements: 0,
                },
            }),
            sources: vec![],
        });
    }
    if raw.count() != request.entities.len() {
        return Err("generator reserved identity count mismatch".into());
    }
    match raw {
        Materialized::Mixed(roots) => {
            super::mixed::bind(assets, table, generation, region, request, roots)
        }
        Materialized::Creature { source, gear } => {
            let assets = assets?;
            let actor = request.entities[0];
            let template = region
                .creatures
                .get(&source.weenie_id)
                .ok_or("missing prepared creature")?
                .as_ref()
                .map_err(Clone::clone)?;
            let gear: Vec<_> = request.entities[1..]
                .iter()
                .copied()
                .zip(gear.iter().cloned())
                .collect();
            if table.is_none() {
                *table = Some(assets.prepare_world_spell_table()?);
            }
            let templates = gear
                .iter()
                .enumerate()
                .map(|(n, (_, i))| (n as u32, Arc::new(i.source.clone())))
                .collect();
            let spells = crate::generator_spell_assets::prepare_generator_spell_rows(
                generation,
                table.as_ref().ok_or("missing client spell table")?.clone(),
                &templates,
            )?;
            let mut shapes = region
                .physical
                .iter()
                .filter_map(|(&id, p)| p.as_ref().ok().map(|p| (id, p.shape.clone())))
                .collect::<BTreeMap<_, _>>();
            for (_, item) in &gear {
                if matches!(item.wielded_location, 0x400000 | 0x800000) {
                    shapes.insert(
                        item.source.weenie_id,
                        assets.prepare_world_item_shape(&item.source)?,
                    );
                }
            }
            let mut loadout =
                crate::generator_equipment::prepare_creature_loadout_with_enchantments(
                    actor,
                    source,
                    &gear,
                    template,
                    &shapes,
                    Some(spells.borrowed()),
                )?;
            let raw_equipment = loadout
                .profile
                .equipment
                .iter()
                .map(|stamp| {
                    let raw = gear
                        .iter()
                        .find(|(id, _)| id.0 == stamp.entity)
                        .ok_or("NPC wielded source absent")?;
                    Ok(bace_combat::preparation::PhysicalEquipmentSource {
                        entity: stamp.entity,
                        revision: stamp.revision,
                        location: stamp.location,
                        weenie: Arc::new(raw.1.source.clone()),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            assets.prepare_npc_loadout_motions(source, &mut loadout, &raw_equipment, generation)?;
            let mut sources = Vec::new();
            for ((id, raw), item) in gear.iter().zip(&loadout.items) {
                if *id != item.id {
                    return Err("generated equipment projection identity mismatch".into());
                }
                sources.push(source_for(
                    item,
                    raw.source.clone(),
                    key.generator.content_revision,
                    raw.source_destination,
                ));
            }
            if let GeneratorDestination::Contain { container } = request.intent.destination {
                let slots = request
                    .next_slots
                    .ok_or("missing owner container slot snapshot")?;
                let (mut root, _) = crate::generator_items::prepare_inventory_item(
                    source,
                    actor,
                    1,
                    ItemPlace::World,
                )?;
                root.place = ItemPlace::Contained {
                    container,
                    slot: if root.pack_slot { slots.1 } else { slots.0 },
                    equipped: 0,
                };
                let mut root_source = (**source).clone();
                set(
                    &mut root_source.properties.instance_ids,
                    6,
                    key.generator.entity.0,
                );
                let mut root_metadata =
                    source_for(&root, root_source, key.generator.content_revision, None);
                root_metadata.item.construction = Some(super::construction::freeze(
                    actor,
                    source.weenie_type,
                    &request.intent,
                    &gear,
                    &loadout,
                )?);
                sources.insert(0, root_metadata);
                return Ok(Ready {
                    action: GeneratorAction::AdmitContainedCreature {
                        key,
                        prepared: Box::new(bace_simulation::PreparedContainedCreature {
                            root,
                            loadout: Box::new(loadout),
                            weenie_type: source.weenie_type,
                        }),
                    },
                    sources,
                });
            }
            Ok(Ready {
                action: GeneratorAction::AdmitCreature {
                    key,
                    loadout: Box::new(loadout),
                },
                sources,
            })
        }
        Materialized::Items(raw) => {
            let mut assets = assets;
            bind_items(request, raw, |source| {
                assets
                    .as_mut()
                    .map_err(|e| e.clone())
                    .and_then(|a| a.prepare_world_item_shape(source))
            })
        }
        Materialized::NestedItems { items, .. } => {
            let mut part = request.clone();
            part.entities.truncate(items.len());
            let mut assets = assets;
            bind_items(&part, items, |source| {
                assets
                    .as_mut()
                    .map_err(|e| e.clone())
                    .and_then(|a| a.prepare_world_item_shape(source))
            })
        }
    }
}
pub(super) fn bind_items(
    request: &GeneratorHostRequest,
    raw: &[PreparedContainerItem],
    mut shape: impl FnMut(&WeenieV1) -> Result<Arc<bace_physics::CollisionShape>, String>,
) -> Result<Ready, String> {
    let key = request.intent.key;
    if matches!(
        request.intent.destination,
        GeneratorDestination::Shop { .. }
    ) {
        return super::stock::bind(request, raw);
    }
    let mut items = Vec::new();
    let mut containers = Vec::new();
    let mut roots = Vec::new();
    let mut shapes = Vec::new();
    let mut sources = Vec::new();
    let (mut main, mut pack) = request.next_slots.unwrap_or((0, 0));
    for (index, (id, raw)) in request.entities.iter().copied().zip(raw).enumerate() {
        let pack_slot = raw
            .source
            .properties
            .bools
            .iter()
            .any(|p| p.id == 81 && p.value);
        let place = if let Some(parent) = raw.parent_index {
            if parent >= index {
                return Err("invalid generated tree ordering".into());
            }
            ItemPlace::Contained {
                container: request.entities[parent],
                slot: raw.inventory_slot,
                equipped: 0,
            }
        } else {
            roots.push(id);
            match request.intent.destination {
                GeneratorDestination::Contain { container } => {
                    if request.next_slots.is_none() {
                        return Err("missing owner container slot snapshot".into());
                    }
                    let next = if pack_slot { &mut pack } else { &mut main };
                    let slot = *next;
                    *next = next
                        .checked_add(1)
                        .ok_or("generated container slot overflow")?;
                    ItemPlace::Contained {
                        container,
                        slot,
                        equipped: 0,
                    }
                }
                _ => {
                    shapes.push(shape(&raw.source)?);
                    ItemPlace::World
                }
            }
        };
        let mut template = raw.source.clone();
        let parent = raw
            .generator_parent_index
            .map_or(key.generator.entity, |p| request.entities[p]);
        set(&mut template.properties.instance_ids, 6, parent.0);
        let (item, container) =
            crate::generator_items::prepare_inventory_item(&template, id, 1, place)?;
        sources.push(source_for(
            &item,
            template,
            key.generator.content_revision,
            raw.source_destination,
        ));
        items.push(item);
        containers.extend(container);
    }
    Ok(Ready {
        action: GeneratorAction::AdmitItemTrees {
            key,
            items,
            containers,
            roots,
            shapes: (!shapes.is_empty()).then_some(shapes),
        },
        sources,
    })
}
pub(super) fn source_for(
    item: &bace_inventory::InventoryItem,
    source: WeenieV1,
    revision: u64,
    source_destination: Option<u8>,
) -> RegionItemSource {
    let placement = match item.place {
        ItemPlace::Contained {
            container,
            slot,
            equipped,
        } => Some(bace_storage_codec::ItemPlacementV2::Contained {
            container: container.0,
            slot,
            pack_slot: item.pack_slot,
            equipped,
        }),
        _ => None,
    };
    RegionItemSource {
        item: crate::game_inventory::FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination,
            entity: bace_storage_codec::EntitySaveV1 {
                object_id: item.id.0,
                template_revision: revision,
                mutation_revision: item.revision,
                state: source,
            },
            placement,
            enchantments: vec![],
            persisted_version: 0,
        },
        corpse: None,
    }
}
pub(super) fn set<T>(values: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(p) = values.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        values.push(Property { id, value });
        values.sort_by_key(|p| p.id);
    }
}
