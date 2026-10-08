//! Source-ordered item/NPC treasure roots share one exact occurrence receipt.
//! Only incoming immutable graphs are staged; the live inventory is never cloned.
use super::*;
use crate::{
    GeneratorItemAdmission, GeneratorServiceError as G, PreparedMixedGeneratorRoot as Root,
};
use bace_gameplay_api::{
    GeneratorDestination, GeneratorLocation, GeneratorSpawnKey, GeneratorSpawnMember,
    GeneratorSpawnReceipt, GeneratorSpawnResult,
};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use std::collections::{BTreeMap, BTreeSet};
impl Kernel {
    pub fn admit_generated_mixed_trees(
        &mut self,
        key: GeneratorSpawnKey,
        roots: Vec<Root>,
    ) -> Result<GeneratorItemAdmission, G> {
        if roots.is_empty()
            || roots.len() > 1024
            || roots.iter().any(|r| !r.valid_bounds())
            || roots.iter().map(Root::entity_count).sum::<usize>() > 1024
            || roots.iter().map(Root::container_count).sum::<usize>() > 1024
            || roots.iter().map(Root::enchantment_count).sum::<usize>() > 4096
        {
            return Err(G::Invalid);
        }
        let request = self
            .generators
            .requests
            .get(&key)
            .filter(|_| self.generators.submitted.contains(&key))
            .cloned()
            .ok_or(G::Stale)?;
        if !matches!(
            request.intent.destination,
            GeneratorDestination::Default(_)
                | GeneratorDestination::Specific(_)
                | GeneratorDestination::Scatter { .. }
        ) {
            return Err(G::Invalid);
        }
        let (items, containers, owners, ids) = validate(&roots)?;
        if request.entities.len() != ids.len()
            || request.entities.iter().copied().collect::<BTreeSet<_>>() != ids
        {
            return Err(G::Invalid);
        }
        if roots.len() > self.generators.capacity - self.generators.events.len() {
            return Err(G::Capacity);
        }
        let restored_ids: BTreeSet<_> = roots
            .iter()
            .filter_map(|root| match root {
                Root::Creature { loadout, .. }
                    if loadout
                        .combat_assets
                        .as_ref()
                        .is_some_and(|assets| assets.restored_registries.is_some()) =>
                {
                    Some(loadout.items.iter().map(|item| item.id))
                }
                _ => None,
            })
            .flatten()
            .collect();
        let transient = items
            .iter()
            .map(|i| i.id)
            .filter(|id| !restored_ids.contains(id))
            .collect();
        let prepared = self
            .inventory
            .prepare_region_admission(&items, &containers, &transient)
            .map_err(inventory_error)?;
        let mut templates = BTreeMap::new();
        for root in &roots {
            if let Root::Creature {
                root: actor,
                template,
                loadout,
            } = root
            {
                let mut npc = self
                    .generators
                    .templates
                    .get(&(key.generator.content_revision, *template))
                    .cloned()
                    .ok_or(G::Missing)?;
                if npc
                    .physical
                    .as_ref()
                    .is_none_or(|p| p.content_hash != loadout.profile.content_hash)
                    || !self.combat.proposed_equipment_current(
                        &loadout.profile,
                        *actor,
                        prepared.inventory(),
                    )
                {
                    return Err(G::Invalid);
                }
                bace_combat::physical::validate_physical_profile(&loadout.profile)
                    .map_err(|_| G::Invalid)?;
                if npc.combat_assets.is_some() && loadout.combat_assets.is_none() {
                    return Err(G::Missing);
                }
                npc.combat_assets = loadout.combat_assets.clone();
                npc.death_motions = loadout.death_motions.clone();
                npc.physical_motions = loadout.physical_motions.clone();
                npc.locomotion_styles = loadout.locomotion_styles.clone();
                npc.physical = Some(loadout.profile.clone());
                if let Some(loot) = &mut npc.ace_loot {
                    loot.initial_items = loadout.death_items.clone();
                    loot.initial_parents = loadout.death_parents.clone();
                    loot.initial_ids = loadout.death_ids.clone();
                } else if !loadout.death_items.is_empty() {
                    return Err(G::Missing);
                }
                templates.insert(*actor, npc);
            }
        }
        self.preflight_npc_scripts(templates.iter().filter_map(|(&actor, template)| {
            template.script.as_ref().map(|script| (actor, script))
        }))
        .map_err(|_| G::Invalid)?;
        self.preflight_generated_enchantment_batch(roots.iter().filter_map(|root| match root {
            Root::Creature { root, loadout, .. } => Some((*root, loadout.enchantments.as_slice())),
            _ => None,
        }))?;
        let mut inserted = Vec::new();
        let mut npcs = Vec::new();
        let result = (|| {
            let mut failed = BTreeSet::new();
            let mut locations = BTreeMap::new();
            for (ordinal, root) in roots.iter().enumerate() {
                let id = root.entity();
                let location = match root {
                    Root::Item { shape, .. } => self
                        .prepare_generated_world_actor_ordinal(
                            key,
                            id,
                            shape.clone(),
                            ordinal as u32,
                        )
                        .and_then(|actor| {
                            self.world.insert(actor).map_err(|_| G::Placement)?;
                            inserted.push(id);
                            self.generated_root_location(id)
                        }),
                    Root::Creature { .. } => self
                        .stage_generator_npc(
                            &request.intent,
                            id,
                            templates.remove(&id).ok_or(G::Missing)?,
                            ordinal as u32,
                        )
                        .inspect(|_| npcs.push(id)),
                };
                match location {
                    Ok(location) => {
                        locations.insert(id, location);
                    }
                    Err(G::Placement) => {
                        failed.insert(id);
                    }
                    Err(e) => return Err(e),
                }
            }
            let accepted_items: Vec<_> = items
                .into_iter()
                .filter(|i| !failed.contains(&owners[&i.id]))
                .collect();
            let accepted_containers: Vec<_> = containers
                .into_iter()
                .filter(|c| !failed.contains(&owners[&c.id]))
                .collect();
            let transient = accepted_items
                .iter()
                .map(|i| i.id)
                .filter(|id| !restored_ids.contains(id))
                .collect();
            let forest = self
                .inventory
                .prepare_region_admission(&accepted_items, &accepted_containers, &transient)
                .map_err(inventory_error)?;
            let placements: Vec<_> = roots
                .iter()
                .filter(|r| !failed.contains(&r.entity()))
                .flat_map(|r| {
                    let location = locations[&r.entity()];
                    match r {
                        Root::Item { items, .. } => items
                            .iter()
                            .map(|i| (i.id, i.template, location))
                            .collect::<Vec<_>>(),
                        Root::Creature {
                            root,
                            template,
                            loadout,
                        } => std::iter::once((*root, *template, location))
                            .chain(loadout.items.iter().map(|i| (i.id, i.template, location)))
                            .collect(),
                    }
                })
                .collect();
            let nested = self.prepare_nested_generators(&request.intent, &placements)?;
            let admitted: Vec<_> = roots
                .iter()
                .map(Root::entity)
                .filter(|id| !failed.contains(id))
                .collect();
            let mut birth_events = Vec::with_capacity(admitted.len());
            for root in roots.iter().filter(|root| !failed.contains(&root.entity())) {
                let entity = root.entity();
                let template = match root {
                    Root::Item { items, .. } => {
                        items
                            .iter()
                            .find(|item| item.id == entity)
                            .ok_or(G::Invalid)?
                            .template
                    }
                    Root::Creature { template, .. } => *template,
                };
                birth_events.push(crate::GeneratorWorldEvent::Spawned {
                    birth: self.prepare_generator_birth(entity)?,
                    key,
                    entity,
                    template,
                    location: locations[&entity],
                });
            }
            self.confirm_generator_spawn(GeneratorSpawnReceipt {
                key,
                result: GeneratorSpawnResult::Completed {
                    members: admitted
                        .iter()
                        .map(|entity| GeneratorSpawnMember {
                            entity: *entity,
                            contribution: 1,
                        })
                        .collect(),
                    materialized: true,
                    failed_placements: failed.len() as u32,
                },
            })?;
            self.inventory.adopt_region_admission(forest);
            self.adopt_nested_generators(nested);
            let mut accepted_ids = Vec::new();
            for root in roots {
                let id = root.entity();
                if failed.contains(&id) {
                    continue;
                }
                match root {
                    Root::Item { items, .. } => {
                        accepted_ids.extend(items.iter().map(|i| i.id));
                    }
                    Root::Creature { loadout, .. } => {
                        self.confirm_npc_combat_assets(id);
                        accepted_ids.push(id);
                        accepted_ids.extend(loadout.items.iter().map(|i| i.id));
                        self.enqueue_generated_enchantments(id, loadout.enchantments);
                    }
                };
            }
            self.generators.events.extend(birth_events);
            Ok(GeneratorItemAdmission {
                key,
                roots: admitted,
                entities: accepted_ids,
                failed_roots: failed.into_iter().collect(),
            })
        })();
        if result.is_err() {
            for id in inserted {
                self.world.remove(id);
            }
            for id in npcs {
                self.rollback_staged_generator_npc(id)?;
            }
        }
        result
    }
    pub(super) fn rollback_staged_generator_npc(&mut self, id: EntityId) -> Result<(), G> {
        let origin = self.population.generated_origin(id).ok_or(G::Invalid)?;
        self.rollback_npc_script_admission(id)
            .map_err(|_| G::Busy)?;
        self.rollback_npc_combat_assets(id).map_err(|_| G::Busy)?;
        self.combat.retire_actor(id);
        self.population
            .remove_generated(id, origin, &mut self.world)
            .map_err(|_| G::Invalid)
    }
    fn generated_root_location(&self, id: EntityId) -> Result<GeneratorLocation, G> {
        let (cell, state) = self.world.actor_state(id).map_err(|_| G::Invalid)?;
        let p = state.position();
        let heading = state.heading_radians() * 0.5;
        Ok(GeneratorLocation {
            cell: cell.0,
            origin: [p.x, p.y, p.z],
            rotation: [0., 0., heading.sin(), heading.cos()],
        })
    }
}
#[expect(
    clippy::type_complexity,
    reason = "bounded incoming item forest and root ownership index"
)]
fn validate(
    roots: &[Root],
) -> Result<
    (
        Vec<InventoryItem>,
        Vec<InventoryContainer>,
        BTreeMap<EntityId, EntityId>,
        BTreeSet<EntityId>,
    ),
    G,
> {
    let mut items = Vec::new();
    let mut containers = Vec::new();
    let mut owners = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for root in roots {
        let actor = root.entity();
        let restored = matches!(root,Root::Creature{loadout,..} if loadout.combat_assets.as_ref().is_some_and(|assets|assets.restored_registries.is_some()));
        if actor.0 == 0 || actor.0 == u32::MAX {
            return Err(G::Invalid);
        }
        let (child, storage) = match root {
            Root::Creature { loadout, .. } => {
                Kernel::validate_npc_loadout(actor, loadout)?;
                if !ids.insert(actor) {
                    return Err(G::Invalid);
                }
                owners.insert(actor, actor);
                (&loadout.items, &loadout.containers)
            }
            Root::Item {
                items, containers, ..
            } => {
                if items
                    .iter()
                    .filter(|i| i.id == actor && i.place == ItemPlace::World)
                    .count()
                    != 1
                    || containers.iter().any(|c| {
                        c.root_owner.is_some()
                            || !items.iter().any(|i| i.id == c.id && i.is_container)
                    })
                {
                    return Err(G::Invalid);
                }
                for item in items {
                    let mut cursor = item;
                    let mut seen = BTreeSet::new();
                    while cursor.id != actor {
                        if !seen.insert(cursor.id) || seen.len() > 64 {
                            return Err(G::Invalid);
                        }
                        let ItemPlace::Contained {
                            container,
                            equipped: 0,
                            ..
                        } = cursor.place
                        else {
                            return Err(G::Invalid);
                        };
                        cursor = items
                            .iter()
                            .find(|i| i.id == container && i.is_container)
                            .ok_or(G::Invalid)?;
                    }
                }
                (items, containers)
            }
        };
        for item in child {
            if !ids.insert(item.id) || item.revision == 0 || (!restored && item.revision != 1) {
                return Err(G::Invalid);
            }
            owners.insert(item.id, actor);
            items.push(item.clone());
        }
        containers.extend(storage.iter().copied());
    }
    Ok((items, containers, owners, ids))
}
fn inventory_error(error: bace_gameplay_api::InventoryRejection) -> G {
    if error == bace_gameplay_api::InventoryRejection::Capacity {
        G::Capacity
    } else {
        G::Invalid
    }
}

#[cfg(test)]
mod tests;
