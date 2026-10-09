//! Atomic Contain forests share the existing inventory and registry owners.
use super::*;
use crate::PreparedContainedForest;
impl Kernel {
    pub fn admit_contained_forest(
        &mut self,
        key: GeneratorSpawnKey,
        forest: PreparedContainedForest,
    ) -> Result<GeneratorItemAdmission, G> {
        if !forest.valid_bounds() {
            return Err(G::Invalid);
        }
        if self.constructed_creatures.entries.len() + forest.creatures.len() > 128
            || self.constructed_creatures.items
                + forest
                    .creatures
                    .iter()
                    .map(|c| c.loadout.items.len() + 1)
                    .sum::<usize>()
                > 4096
        {
            return Err(G::Capacity);
        }
        let request = self
            .generators
            .requests
            .get(&key)
            .filter(|_| self.generators.submitted.contains(&key))
            .ok_or(G::Stale)?;
        let GeneratorDestination::Contain { container } = request.intent.destination else {
            return Err(G::Invalid);
        };
        let intent = request.intent.clone();
        let landblock = request.landblock;
        let items: BTreeMap<_, _> = forest.items.iter().map(|i| (i.id, i)).collect();
        let ids: BTreeSet<_> = items.keys().copied().collect();
        let roots: BTreeSet<_> = forest.roots.iter().copied().collect();
        if items.len() != forest.items.len()
            || roots.len() != forest.roots.len()
            || ids.len() != request.entities.len()
            || request.entities.iter().any(|id| !ids.contains(id))
            || forest.containers.iter().any(|c| c.root_owner.is_some())
        {
            return Err(G::Invalid);
        }
        for root in &roots {
            if items.get(root).is_none_or(|i| i.revision!=1 || !matches!(i.place,ItemPlace::Contained{container:c,equipped:0,..}if c==container)) {return Err(G::Invalid)}
        }
        let mut owners = BTreeMap::new();
        for item in &forest.items {
            let mut current = item.id;
            let mut seen = BTreeSet::new();
            loop {
                if !seen.insert(current) || seen.len() > 64 {
                    return Err(G::Invalid);
                }
                if roots.contains(&current) {
                    owners.insert(item.id, current);
                    break;
                }
                let ItemPlace::Contained { container, .. } =
                    items.get(&current).ok_or(G::Invalid)?.place
                else {
                    return Err(G::Invalid);
                };
                current = container;
            }
        }
        let graph = self
            .inventory
            .prepare_contained_forest(&forest.roots, &forest.items, &forest.containers)
            .map_err(|_| G::Invalid)?;
        let mut constructions = BTreeMap::new();
        for creature in &forest.creatures {
            let actor = creature.entity;
            // A constructed Creature can be nested below another contained
            // root. It still owns its own loadout, registry and subtype.
            if constructions.contains_key(&actor)
                || self.constructed_creatures.contains(actor)
                || self.world.contains_identity(actor)
            {
                return Err(G::Invalid);
            }
            let root = items.get(&actor).ok_or(G::Invalid)?;
            if !root.is_container
                || root.stack != 1
                || root.revision != 1
                || !matches!(root.place, ItemPlace::Contained { equipped: 0, .. })
            {
                return Err(G::Invalid);
            }
            let children = constructed_descendants(actor, &forest.items, &items)?;
            if children.len() != creature.loadout.items.len()
                || creature
                    .loadout
                    .items
                    .iter()
                    .any(|i| items.get(&i.id).is_none_or(|existing| *existing != i))
            {
                return Err(G::Invalid);
            }
            if creature
                .loadout
                .containers
                .iter()
                .any(|c| !forest.containers.contains(c))
            {
                return Err(G::Invalid);
            }
            let mut template = self
                .generators
                .templates
                .get(&(key.generator.content_revision, root.template))
                .cloned()
                .ok_or(G::Missing)?;
            if template
                .physical
                .as_ref()
                .is_none_or(|p| p.content_hash != creature.loadout.profile.content_hash)
                || !self.combat.proposed_equipment_current(
                    &creature.loadout.profile,
                    actor,
                    graph.inventory(),
                )
            {
                return Err(G::Invalid);
            }
            bace_combat::physical::validate_physical_profile(&creature.loadout.profile)
                .map_err(|_| G::Invalid)?;
            let loadout = &creature.loadout;
            template.physical = Some(loadout.profile.clone());
            template.combat_assets = loadout.combat_assets.clone();
            template.death_motions = loadout.death_motions.clone();
            template.physical_motions = loadout.physical_motions.clone();
            template.locomotion_styles = loadout.locomotion_styles.clone();
            if let Some(loot) = &mut template.ace_loot {
                loot.initial_items = loadout.death_items.clone();
                loot.initial_parents = loadout.death_parents.clone();
                loot.initial_ids = loadout.death_ids.clone();
            } else if !loadout.death_items.is_empty() {
                return Err(G::Missing);
            }
            constructions.insert(
                actor,
                ConstructedCreature {
                    landblock,
                    intent: intent.clone(),
                    template,
                    children,
                    durable: false,
                    acquired: false,
                },
            );
        }
        self.preflight_generated_enchantment_batch(
            forest
                .creatures
                .iter()
                .map(|c| (c.entity, c.loadout.enchantments.as_slice())),
        )?;
        let parent = self
            .generators
            .machines
            .get(&key.generator.entity)
            .ok_or(G::Stale)?
            .definition()
            .location;
        let placements: Vec<_> = forest
            .items
            .iter()
            .map(|i| (i.id, i.template, parent))
            .collect();
        let nested = self.prepare_nested_generators(&intent, &placements)?;
        if nested
            .iter()
            .any(|m| constructions.contains_key(&owners[&m.definition().identity.entity]))
        {
            return Err(G::Missing);
        }
        self.confirm_generator_spawn(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Completed {
                members: forest
                    .roots
                    .iter()
                    .map(|&entity| GeneratorSpawnMember {
                        entity,
                        contribution: 1,
                    })
                    .collect(),
                materialized: true,
                failed_placements: 0,
            },
        })?;
        self.inventory.adopt_region_admission(graph);
        for (actor, construction) in constructions {
            self.inventory.mark_constructed(actor);
            self.constructed_creatures.items += construction.children.len() + 1;
            self.constructed_creatures
                .entries
                .insert(actor, construction);
        }
        self.adopt_nested_generators(nested);
        for creature in forest.creatures {
            self.enqueue_generated_enchantments(creature.entity, creature.loadout.enchantments);
        }
        Ok(GeneratorItemAdmission {
            key,
            roots: forest.roots,
            entities: forest.items.iter().map(|i| i.id).collect(),
            failed_roots: vec![],
        })
    }
}

fn constructed_descendants(
    actor: EntityId,
    candidates: &[bace_inventory::InventoryItem],
    items: &BTreeMap<EntityId, &bace_inventory::InventoryItem>,
) -> Result<Vec<EntityId>, G> {
    let mut children = Vec::new();
    for item in candidates {
        if item.id == actor {
            continue;
        }
        let mut current = item.id;
        for _ in 0..64 {
            let ItemPlace::Contained { container, .. } =
                items.get(&current).ok_or(G::Invalid)?.place
            else {
                break;
            };
            if container == actor {
                children.push(item.id);
                break;
            }
            if !items.contains_key(&container) {
                break;
            }
            current = container;
        }
    }
    Ok(children)
}
