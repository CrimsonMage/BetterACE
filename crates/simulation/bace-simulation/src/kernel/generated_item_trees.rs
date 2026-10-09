//! One root contributes one generator member; generated contents remain ordinary
//! descendants. Failed world placement discards only that whole root's tree, as
//! ACE GeneratorProfile.Spawn does for each materialized treasure root.
use super::*;
use crate::{GeneratorItemAdmission, GeneratorServiceError as G};
use bace_gameplay_api::{
    GeneratorDestination, GeneratorSpawnKey, GeneratorSpawnMember, GeneratorSpawnReceipt,
    GeneratorSpawnResult,
};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
impl Kernel {
    pub fn admit_generated_item_trees(
        &mut self,
        key: GeneratorSpawnKey,
        items: &[InventoryItem],
        containers: &[InventoryContainer],
        roots: &[EntityId],
        shapes: Option<&[Arc<bace_physics::CollisionShape>]>,
    ) -> Result<GeneratorItemAdmission, G> {
        if items.is_empty()
            || items.len() > 1024
            || containers.len() > 1024
            || roots.is_empty()
            || roots.len() > items.len()
        {
            return Err(G::Invalid);
        }
        let request = self.generators.requests.get(&key).ok_or(G::Stale)?;
        let ids: BTreeSet<_> = items.iter().map(|i| i.id).collect();
        let root_ids: BTreeSet<_> = roots.iter().copied().collect();
        let rejected =
            |stage: &'static str,
             entity: EntityId,
             inventory: Option<bace_gameplay_api::InventoryRejection>| {
                G::GeneratedItemForest {
                    stage,
                    entity,
                    slot: items
                        .iter()
                        .find(|item| item.id == entity)
                        .and_then(|item| {
                            if let ItemPlace::Contained { slot, .. } = item.place {
                                Some(slot)
                            } else {
                                None
                            }
                        }),
                    inventory,
                }
            };
        if !self.generators.submitted.contains(&key)
            || ids.len() != items.len()
            || root_ids.len() != roots.len()
            || !root_ids.is_subset(&ids)
            || request.entities.iter().copied().collect::<BTreeSet<_>>() != ids
            || request.entities.len() != items.len()
            || containers.iter().any(|c| {
                c.root_owner.is_some() || !items.iter().any(|i| i.id == c.id && i.is_container)
            })
        {
            return Err(rejected("identity partition", roots[0], None));
        }
        let intent = request.intent.clone();
        let map: BTreeMap<_, _> = items.iter().map(|i| (i.id, i)).collect();
        let mut owners = BTreeMap::new();
        for item in items {
            let mut cursor = item;
            let mut seen = BTreeSet::new();
            loop {
                if !seen.insert(cursor.id) || seen.len() > 64 {
                    return Err(rejected("containment cycle or depth", item.id, None));
                }
                if root_ids.contains(&cursor.id) {
                    owners.insert(item.id, cursor.id);
                    break;
                }
                let ItemPlace::Contained {
                    container,
                    equipped: 0,
                    ..
                } = cursor.place
                else {
                    return Err(rejected("non-contained descendant", item.id, None));
                };
                cursor = map
                    .get(&container)
                    .copied()
                    .ok_or_else(|| rejected("missing contained parent", item.id, None))?;
                if !cursor.is_container {
                    return Err(rejected("non-container parent", item.id, None));
                }
            }
        }
        let world = match intent.destination {
            GeneratorDestination::Contain { container } => {
                if shapes.is_some() || self.inventory.reserved(container)
                    || roots.iter().any(|id| !matches!(map[id].place, ItemPlace::Contained { container: c, equipped: 0, .. } if c == container))
                { return Err(rejected("contain destination", roots[0], None)); }
                false
            }
            GeneratorDestination::Shop { .. } => return Err(G::Invalid),
            _ => {
                if shapes.is_none_or(|s| s.len() != roots.len())
                    || roots.iter().any(|id| map[id].place != ItemPlace::World)
                {
                    return Err(G::Invalid);
                }
                true
            }
        };
        // Validate the complete input before accepting even one physical root.
        // The world path stages only the bounded incoming forest, not live state.
        let mut contained = None;
        if world {
            self.inventory
                .prepare_region_admission(items, containers, &ids)
                .map_err(inventory_error)?;
        } else {
            let mut next = self.inventory.clone();
            next.register_generated(items, containers)
                .map_err(|error| {
                    rejected("contained inventory registration", roots[0], Some(error))
                })?;
            contained = Some(next);
        }
        let mut inserted = Vec::new();
        let mut births = super::generator_births::Births::new();
        let mut failed = BTreeSet::new();
        let result = (|| {
            if let Some(shapes) = shapes {
                for (id, shape) in roots.iter().zip(shapes) {
                    match self.prepare_generated_world_actor(key, *id, shape.clone()) {
                        Ok(actor) => {
                            let birth = self.prepare_staged_generator_birth(&actor)?;
                            self.world.insert(actor).map_err(|_| G::Placement)?;
                            births.insert(*id, birth);
                            inserted.push(*id);
                        }
                        Err(G::Placement) => {
                            failed.insert(*id);
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
            let accepted: Vec<_> = items
                .iter()
                .filter(|i| !failed.contains(&owners[&i.id]))
                .cloned()
                .collect();
            let accepted_ids: BTreeSet<_> = accepted.iter().map(|i| i.id).collect();
            let accepted_containers: Vec<_> = containers
                .iter()
                .filter(|c| accepted_ids.contains(&c.id))
                .copied()
                .collect();
            let forest = if world {
                Some(
                    self.inventory
                        .prepare_region_admission(&accepted, &accepted_containers, &accepted_ids)
                        .map_err(inventory_error)?,
                )
            } else {
                None
            };
            let parent = self
                .generators
                .machines
                .get(&key.generator.entity)
                .ok_or(G::Stale)?
                .definition()
                .location;
            let placements: Vec<_> = accepted
                .iter()
                .map(|item| {
                    let location =
                        if let Ok((cell, state)) = self.world.actor_state(owners[&item.id]) {
                            let position = state.position();
                            let heading = state.heading_radians() * 0.5;
                            bace_gameplay_api::GeneratorLocation {
                                cell: cell.0,
                                origin: [position.x, position.y, position.z],
                                rotation: [0., 0., heading.sin(), heading.cos()],
                            }
                        } else {
                            parent
                        };
                    (item.id, item.template, location)
                })
                .collect();
            let nested = self.prepare_nested_generators(&intent, &placements)?;
            let public: Vec<_> = placements
                .iter()
                .filter(|(id, _, _)| root_ids.contains(id) && world)
                .copied()
                .collect();
            if public.len() > self.generators.capacity - self.generators.events.len() {
                return Err(G::Capacity);
            }
            let mut events = Vec::with_capacity(public.len());
            for (entity, template, location) in public {
                events.push(crate::GeneratorWorldEvent::Spawned {
                    birth: births.remove(&entity).ok_or(G::Invalid)?,
                    key,
                    entity,
                    template,
                    location,
                });
            }
            let admitted_roots: Vec<_> = roots
                .iter()
                .copied()
                .filter(|id| !failed.contains(id))
                .collect();
            self.confirm_generator_spawn(GeneratorSpawnReceipt {
                key,
                result: GeneratorSpawnResult::Completed {
                    members: admitted_roots
                        .iter()
                        .map(|&entity| GeneratorSpawnMember {
                            entity,
                            contribution: 1,
                        })
                        .collect(),
                    materialized: true,
                    failed_placements: failed.len() as u32,
                },
            })?;
            if let Some(forest) = forest {
                self.inventory.adopt_region_admission(forest);
            } else if let Some(next) = contained.take() {
                self.inventory = next;
            }
            self.adopt_nested_generators(nested);
            self.generators.events.extend(events);
            Ok(GeneratorItemAdmission {
                key,
                roots: admitted_roots,
                entities: accepted.iter().map(|i| i.id).collect(),
                failed_roots: roots
                    .iter()
                    .copied()
                    .filter(|id| failed.contains(id))
                    .collect(),
            })
        })();
        if result.is_err() {
            for id in inserted {
                self.world.remove(id);
            }
        }
        result
    }
}
fn inventory_error(error: bace_gameplay_api::InventoryRejection) -> G {
    if error == bace_gameplay_api::InventoryRejection::Capacity {
        G::Capacity
    } else {
        G::Invalid
    }
}
