//! One atomic owner transition for prepared region roots, geometry and machines.
use super::*;
impl Kernel {
    pub fn admit_generator_region(
        &mut self,
        region: PreparedGeneratorRegion,
    ) -> Result<(), GeneratorServiceError> {
        self.admit_generator_region_inner(
            &region,
            &mut super::region_items::PreparedWorldRegionItems::default(),
            false,
        )
    }
    pub(in crate::kernel) fn admit_generator_region_inner(
        &mut self,
        region: &PreparedGeneratorRegion,
        restored: &mut super::region_items::PreparedWorldRegionItems,
        refresh: bool,
    ) -> Result<(), GeneratorServiceError> {
        self.validate_world_region_items(region.landblock, restored)?;
        if region.epoch == 0
            || region.revision == 0
            || self.generators.root.is_none()
            || self
                .generators
                .activation_fences
                .get(&region.landblock)
                .is_some_and(|(epoch, revision)| {
                    if refresh {
                        *epoch != region.epoch || *revision >= region.revision
                    } else {
                        *epoch >= region.epoch
                    }
                })
            || region.roots.len() > 4096
            || region.containers.len() > 4096
        {
            return Err(GeneratorServiceError::Stale);
        }
        if region.definitions.len() > self.generators.capacity - self.generators.machines.len()
            || region.templates.len() > 4096
            || region.creatures.len() > 4096
        {
            return Err(GeneratorServiceError::Capacity);
        }
        let mut machines = std::collections::BTreeMap::new();
        for definition in &region.definitions {
            let id = definition.identity.entity;
            if definition.location.cell >> 16 != u32::from(region.landblock)
                || definition.identity.incarnation != region.epoch
                || definition.identity.content_revision != region.revision
                || self.generators.machines.contains_key(&id)
                || machines.contains_key(&id)
                || !region
                    .roots
                    .iter()
                    .any(|root| root.entity == id && root.location == definition.location)
            {
                return Err(GeneratorServiceError::Stale);
            }
            let clock = self.generator_clock(definition.event.as_deref())?;
            machines.insert(
                id,
                GeneratorMachine::new(definition.clone(), clock, GeneratorLimits::default())?,
            );
        }
        // Cold durable companions occupy their authored generator profiles
        // before any fresh population work can be scheduled. Their saved
        // creature and equipment identities remain under the inventory owner.
        for creature in &restored.constructed {
            let generator = creature.intent.key.generator.entity;
            let machine = machines
                .get_mut(&generator)
                .ok_or(GeneratorServiceError::Stale)?;
            machine.adopt_restored_contained_member(
                creature.intent.key.profile_id,
                creature.actor,
                creature.intent.profile.weenie_class_id,
            )?;
        }
        let vendors = machines
            .values()
            .filter(|m| m.definition().kind == GeneratorKind::Vendor)
            .count();
        if vendors > 4096 - self.generated_vendors.len()
            || machines.values().any(|m| {
                m.definition().kind == GeneratorKind::Vendor
                    && self
                        .generated_vendors
                        .contains_key(&m.definition().identity.entity)
            })
        {
            return Err(GeneratorServiceError::Capacity);
        }
        let mut definitions = std::collections::BTreeMap::new();
        for (template, definition) in &region.templates {
            let template = *template;
            let key = (region.revision, template);
            if let Some(existing) = self.generators.definitions.get(&key) {
                if **existing != **definition {
                    return Err(GeneratorServiceError::Stale);
                }
                continue;
            }
            if template == 0
                || definition.identity.content_revision != region.revision
                || definitions.insert(key, definition.clone()).is_some()
            {
                return Err(GeneratorServiceError::Stale);
            }
        }
        let mut creatures = std::collections::BTreeMap::new();
        for (template, creature) in &region.creatures {
            let template = *template;
            let key = (region.revision, template);
            if let Some(profile) = &creature.physical {
                bace_combat::physical::validate_physical_profile(profile)
                    .map_err(|_| GeneratorServiceError::Invalid)?;
            }
            if self.generators.templates.contains_key(&key) {
                continue;
            }
            if template == 0
                || creature.loot.is_some() == creature.ace_loot.is_some()
                || creatures.insert(key, creature.clone()).is_some()
            {
                return Err(GeneratorServiceError::Stale);
            }
        }
        if definitions.len() > 4096 - self.generators.definitions.len()
            || creatures.len() > 4096 - self.generators.templates.len()
        {
            return Err(GeneratorServiceError::Capacity);
        }
        let mut items = restored.items.clone();
        let mut containers = restored.containers.clone();
        let mut transient = std::collections::BTreeSet::new();
        for container in &region.containers {
            if !region.roots.iter().any(|root| root.entity == container.id)
                || container.root_owner.is_some()
            {
                return Err(GeneratorServiceError::Invalid);
            }
            containers.push(*container);
        }
        for root in &region.roots {
            if restored.items.iter().any(|i| i.id == root.entity) {
                return Err(GeneratorServiceError::Stale);
            }
            if let Some(loadout) = &root.loadout {
                Self::validate_npc_loadout(root.entity, loadout)?;
                if loadout.items.iter().any(|item| {
                    self.world.contains_identity(item.id)
                        || self.generator_reserves_identity(item.id)
                        || self.population.reserves_identity(item.id)
                        || self.magic.reserves_identity(item.id)
                }) || root
                    .creature
                    .as_ref()
                    .and_then(|c| c.physical.as_ref())
                    .is_none_or(|p| p.content_hash != loadout.profile.content_hash)
                {
                    return Err(GeneratorServiceError::Invalid);
                }
                items.extend(loadout.items.iter().cloned());
                containers.extend(loadout.containers.iter().copied());
                if loadout
                    .combat_assets
                    .as_ref()
                    .is_none_or(|assets| assets.restored_registries.is_none())
                {
                    transient.extend(loadout.items.iter().map(|i| i.id));
                }
            }
        }
        let inventory = self
            .inventory
            .prepare_region_admission(&items, &containers, &transient)
            .map_err(|_| GeneratorServiceError::Invalid)?;
        for root in &region.roots {
            if let Some(loadout) = &root.loadout
                && !self.combat.proposed_equipment_current(
                    &loadout.profile,
                    root.entity,
                    inventory.inventory(),
                )
            {
                return Err(GeneratorServiceError::Invalid);
            }
        }
        let mut combat_assets = Vec::new();
        for root in &region.roots {
            if let Some(creature) = &root.creature {
                let assets = root
                    .loadout
                    .as_ref()
                    .map_or(creature.combat_assets.as_ref(), |l| {
                        l.combat_assets.as_ref()
                    });
                if creature.combat_assets.is_some() && assets.is_none() {
                    return Err(GeneratorServiceError::Missing);
                }
                if let Some(assets) = assets {
                    if root.loadout.is_none() && !assets.registry_items.is_empty() {
                        return Err(GeneratorServiceError::Invalid);
                    }
                    if let Some(loadout) = &root.loadout {
                        let ids: std::collections::BTreeSet<_> =
                            loadout.items.iter().map(|item| item.id).collect();
                        if assets.registry_items.len() != ids.len()
                            || assets
                                .registry_items
                                .iter()
                                .copied()
                                .collect::<std::collections::BTreeSet<_>>()
                                != ids
                        {
                            return Err(GeneratorServiceError::Invalid);
                        }
                    }
                    let physical = root
                        .loadout
                        .as_ref()
                        .map(|l| &l.profile)
                        .or(creature.physical.as_ref())
                        .ok_or(GeneratorServiceError::Missing)?;
                    if **physical != *assets.physical {
                        return Err(GeneratorServiceError::Invalid);
                    }
                    combat_assets.push((root.entity, assets.clone()));
                }
            }
        }
        self.preflight_npc_scripts(region.roots.iter().filter_map(|root| {
            root.script
                .as_ref()
                .or_else(|| {
                    root.creature
                        .as_ref()
                        .and_then(|creature| creature.script.as_ref())
                })
                .map(|script| (root.entity, script))
        }))
        .map_err(|_| GeneratorServiceError::Invalid)?;
        self.preflight_npc_combat_assets(&combat_assets)
            .map_err(|_| GeneratorServiceError::Invalid)?;
        let death_assets: Vec<_> = region
            .roots
            .iter()
            .filter_map(|root| {
                root.creature.as_ref().map(|creature| {
                    (
                        root.entity,
                        root.loadout
                            .as_ref()
                            .map_or(&creature.death_motions, |l| &l.death_motions)
                            .clone(),
                    )
                })
            })
            .filter(|(_, rows)| !rows.is_empty())
            .collect();
        if death_assets.len() > self.world.remaining_death_motion_actors() {
            return Err(GeneratorServiceError::Capacity);
        }
        for (actor, rows) in &death_assets {
            self.world
                .validate_death_motions(*actor, rows)
                .map_err(|_| GeneratorServiceError::Invalid)?;
        }
        let mut motion_assets = Vec::new();
        for root in &region.roots {
            if let Some(creature) = &root.creature
                && creature.physical.is_some()
            {
                let (styles, motions) = root.loadout.as_ref().map_or(
                    (&creature.locomotion_styles, &creature.physical_motions),
                    |l| (&l.locomotion_styles, &l.physical_motions),
                );
                self.world
                    .validate_locomotion_styles(root.entity, styles)
                    .map_err(|_| GeneratorServiceError::Capacity)?;
                self.combat
                    .validate_physical_motions(root.entity, motions)
                    .map_err(|_| GeneratorServiceError::Invalid)?;
                motion_assets.push((root.entity, styles.clone(), motions.clone()));
            }
        }
        if motion_assets.len() > self.world.remaining_locomotion_style_actors() {
            return Err(GeneratorServiceError::Capacity);
        }
        let geometry = if let Some(current) = self.world.geometry() {
            Arc::new(
                current
                    .replace_landblock(region.landblock, &region.geometry)
                    .map_err(|_| GeneratorServiceError::Geometry)?,
            )
        } else {
            region.geometry.clone()
        };
        let npcs = self
            .population
            .prepare_region_npcs(&region.roots, region.epoch, region.revision, self.tick)
            .map_err(|_| GeneratorServiceError::Invalid)?;
        let profiles: Vec<_> = region
            .roots
            .iter()
            .filter_map(|r| {
                r.loadout
                    .as_ref()
                    .map(|l| l.profile.clone())
                    .or_else(|| r.creature.as_ref().and_then(|c| c.physical.clone()))
                    .map(|p| (r.entity, p))
            })
            .collect();
        self.combat
            .prepare_physical_admission(&profiles)
            .map_err(|_| GeneratorServiceError::Capacity)?;
        self.preflight_generated_enchantment_batch(region.roots.iter().filter_map(|r| {
            r.loadout
                .as_ref()
                .map(|l| (r.entity, l.enchantments.as_slice()))
        }))?;
        let enchantments: Vec<_> = region
            .roots
            .iter()
            .filter_map(|r| {
                r.loadout
                    .as_ref()
                    .map(|l| (r.entity, l.enchantments.clone()))
            })
            .collect();
        let mut actors = Vec::with_capacity(region.roots.len());
        for root in &region.roots {
            if self.inventory.item(root.entity).is_some()
                || self.population.reserves_identity(root.entity)
                || self.magic.reserves_identity(root.entity)
                || self.generators.reserves(root.entity)
            {
                return Err(GeneratorServiceError::Stale);
            }
            let q = root.location.rotation;
            if q[0].abs() > 0.0002
                || q[1].abs() > 0.0002
                || !(0.999..=1.001).contains(&q.iter().map(|v| v * v).sum::<f32>())
            {
                return Err(GeneratorServiceError::Geometry);
            }
            let mut body = bace_physics::Body::spawn_geometry(
                &geometry,
                bace_physics::GeometrySpawn {
                    cell: root.location.cell,
                    position: bace_geometry::Vec3::new(
                        root.location.origin[0],
                        root.location.origin[1],
                        root.location.origin[2],
                    ),
                    shape: root.shape.clone(),
                    capabilities: root.creature.as_ref().map_or(
                        bace_motion::Capabilities {
                            speed: 0.0,
                            jump_impulse: 0.0,
                        },
                        |c| c.blueprint.capabilities,
                    ),
                    heading: 2.0 * q[2].atan2(q[3]),
                    maximum_turn_rate: root
                        .creature
                        .as_ref()
                        .map_or(0.0, |c| c.geometry.maximum_turn_rate),
                },
            )
            .map_err(|_| GeneratorServiceError::Geometry)?;
            if let Some(creature) = &root.creature {
                body.adopt_animated_style(creature.geometry.locomotion.clone(), true)
                    .map_err(|_| GeneratorServiceError::Geometry)?;
                body.refresh_locomotion(
                    &creature.geometry.locomotion.profile,
                    creature.geometry.run_rate,
                )
                .map_err(|_| GeneratorServiceError::Geometry)?;
            }
            actors.push(bace_entity::Actor {
                id: root.entity,
                cell: bace_types::CellId(root.location.cell),
                body,
            });
        }
        let mut world_roots = Vec::with_capacity(restored.roots.len());
        for root in &restored.roots {
            let q = root.location.rotation;
            if q[0].abs() > 0.0002
                || q[1].abs() > 0.0002
                || !(0.999..=1.001).contains(&q.iter().map(|v| v * v).sum::<f32>())
            {
                return Err(GeneratorServiceError::Geometry);
            }
            let body = bace_physics::Body::spawn_geometry(
                &geometry,
                bace_physics::GeometrySpawn {
                    cell: root.location.cell,
                    position: bace_geometry::Vec3::new(
                        root.location.origin[0],
                        root.location.origin[1],
                        root.location.origin[2],
                    ),
                    shape: root.shape.clone(),
                    capabilities: bace_motion::Capabilities {
                        speed: 0.0,
                        jump_impulse: 0.0,
                    },
                    heading: 2.0 * q[2].atan2(q[3]),
                    maximum_turn_rate: 0.0,
                },
            )
            .map_err(|_| GeneratorServiceError::Geometry)?;
            world_roots.push((
                bace_entity::Actor {
                    id: root.entity,
                    cell: bace_types::CellId(root.location.cell),
                    body,
                },
                root.corpse.as_ref().map(|c| c.state.clone()),
            ));
        }
        self.world
            .install_geometry_with_world_items(geometry, actors, world_roots)
            .map_err(|_| GeneratorServiceError::Geometry)?;
        self.population.adopt_region_npcs(npcs, &mut self.world);
        self.combat.adopt_physical_admission(profiles);
        for (actor, assets) in combat_assets {
            self.adopt_npc_combat_assets(actor, assets)
                .expect("same-owner NPC combat join preflight");
            self.confirm_npc_combat_assets(actor);
        }
        for (actor, rows) in death_assets {
            self.world
                .register_death_motions(actor, rows)
                .expect("preflighted death assets");
        }
        for (actor, styles, motions) in motion_assets {
            self.world
                .register_locomotion_styles(actor, styles)
                .expect("preflighted actor style slots");
            for (motion, speed, chain) in motions {
                self.combat
                    .register_physical_motion(actor, motion, speed, chain)
                    .expect("preflighted physical programs");
            }
        }
        self.inventory.adopt_region_admission(inventory);
        for root in &region.roots {
            if let Some(script) = root.script.as_ref().or_else(|| {
                root.creature
                    .as_ref()
                    .and_then(|creature| creature.script.as_ref())
            }) {
                self.admit_npc_script(root.entity, script.clone())
                    .expect("same-owner static script preflight");
            }
        }

        self.adopt_world_region_items(restored);
        for (actor, entries) in enchantments {
            self.enqueue_generated_enchantments(actor, entries);
        }
        for machine in machines.values() {
            self.adopt_generator_vendor(machine);
        }
        self.generators.machines.extend(machines);
        self.generators.definitions.extend(definitions);
        self.generators.templates.extend(creatures);
        self.generators
            .activation_fences
            .insert(region.landblock, (region.epoch, region.revision));
        Ok(())
    }
}
