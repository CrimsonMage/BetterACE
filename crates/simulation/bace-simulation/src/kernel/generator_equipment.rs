//! Creature/body/equipment admission is one generator receipt transition.
use super::*;
impl Kernel {
    pub fn admit_generated_creature(
        &mut self,
        key: GeneratorSpawnKey,
        loadout: PreparedNpcLoadout,
    ) -> Result<(), GeneratorServiceError> {
        let request = self
            .generators
            .requests
            .get(&key)
            .filter(|_| self.generators.submitted.contains(&key))
            .cloned()
            .ok_or(GeneratorServiceError::Stale)?;
        let (&actor, children) = request
            .entities
            .split_first()
            .ok_or(GeneratorServiceError::Invalid)?;
        Self::validate_npc_loadout(actor, &loadout)?;
        if children.len() != loadout.items.len()
            || loadout.items.iter().any(|i| !children.contains(&i.id))
        {
            return Err(GeneratorServiceError::Invalid);
        }
        let mut template = self
            .generators
            .templates
            .get(&(
                key.generator.content_revision,
                request.intent.profile.weenie_class_id,
            ))
            .cloned()
            .ok_or(GeneratorServiceError::Missing)?;
        if (!template.requires_equipment && !self.generators.prepared_births)
            || template
                .physical
                .as_ref()
                .is_none_or(|p| p.content_hash != loadout.profile.content_hash)
        {
            return Err(GeneratorServiceError::Stale);
        }
        if !matches!(
            request.intent.destination,
            GeneratorDestination::Default(_)
                | GeneratorDestination::Specific(_)
                | GeneratorDestination::Scatter { .. }
        ) {
            return Err(GeneratorServiceError::Invalid);
        }
        if self.generators.events.len() == self.generators.capacity {
            return Err(GeneratorServiceError::Capacity);
        }
        let mut inventory = self.inventory.clone();
        if loadout.items.is_empty() {
            inventory
                .register_container(loadout.containers[0])
                .map_err(|_| GeneratorServiceError::Capacity)?;
        } else {
            inventory
                .register_generated(&loadout.items, &loadout.containers)
                .map_err(|_| GeneratorServiceError::Capacity)?;
        }
        if !self
            .combat
            .proposed_equipment_current(&loadout.profile, actor, &inventory)
        {
            return Err(GeneratorServiceError::Invalid);
        }
        bace_combat::physical::validate_physical_profile(&loadout.profile)
            .map_err(|_| GeneratorServiceError::Invalid)?;
        if template.combat_assets.is_some() && loadout.combat_assets.is_none() {
            return Err(GeneratorServiceError::Missing);
        }
        template.combat_assets = loadout.combat_assets;
        template.death_motions = loadout.death_motions;
        template.physical_motions = loadout.physical_motions;
        template.locomotion_styles = loadout.locomotion_styles;
        template.physical = Some(loadout.profile);
        template
            .ace_loot
            .as_mut()
            .ok_or(GeneratorServiceError::Missing)?
            .initial_items = loadout.death_items;
        template
            .ace_loot
            .as_mut()
            .ok_or(GeneratorServiceError::Missing)?
            .initial_parents = loadout.death_parents;
        let mut machine = self
            .generators
            .machines
            .get(&key.generator.entity)
            .cloned()
            .ok_or(GeneratorServiceError::Stale)?;
        let transition = machine.acknowledge(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Completed {
                members: vec![GeneratorSpawnMember {
                    entity: actor,
                    contribution: 1,
                }],
                materialized: true,
                failed_placements: 0,
            },
        })?;
        if transition.effects.len() > self.generators.capacity - self.generators.effects.len() {
            return Err(GeneratorServiceError::Capacity);
        }
        self.preflight_generated_enchantments(actor, &loadout.enchantments)?;
        template
            .ace_loot
            .as_mut()
            .ok_or(GeneratorServiceError::Missing)?
            .initial_ids = loadout.death_ids;
        let location = self.spawn_generator_npc(&request.intent, actor, template)?;
        let birth = match self.prepare_generator_birth(actor) {
            Ok(birth) => birth,
            Err(error) => {
                self.rollback_staged_generator_npc(actor)?;
                return Err(error);
            }
        };
        self.inventory = inventory;
        self.enqueue_generated_enchantments(actor, loadout.enchantments);
        self.generators
            .adopt(key.generator.entity, machine, transition)?;
        self.confirm_npc_combat_assets(actor);
        self.generators.requests.remove(&key);
        self.generators.submitted.remove(&key);
        self.generators
            .events
            .push_back(GeneratorWorldEvent::Spawned {
                birth,
                key,
                entity: actor,
                template: request.intent.profile.weenie_class_id,
                location,
            });
        Ok(())
    }
    pub(in crate::kernel) fn validate_npc_loadout(
        actor: EntityId,
        loadout: &PreparedNpcLoadout,
    ) -> Result<(), GeneratorServiceError> {
        use bace_inventory::ItemPlace;
        if loadout.items.len() > 1024
            || loadout.containers.len() > 1025
            || loadout.death_items.len() > 256
            || loadout.death_items.len() != loadout.death_parents.len()
            || loadout.death_ids.len() != loadout.death_items.len()
            || loadout.death_parents.iter().enumerate().any(|(i, p)| {
                p.is_some_and(|p| {
                    p >= i
                        || !loadout.death_items[p]
                            .properties
                            .ints
                            .iter()
                            .any(|x| x.id == 6)
                })
            })
            || loadout.containers.iter().filter(|c| c.id == actor).count() != 1
            || loadout.containers.iter().any(|c| {
                c.root_owner.is_some()
                    || (c.id != actor
                        && !loadout.items.iter().any(|i| i.id == c.id && i.is_container))
            })
        {
            return Err(GeneratorServiceError::Invalid);
        }
        if loadout.death_ids.len()!=loadout.death_items.len() || loadout.death_ids.iter().any(|id|!loadout.items.iter().any(|i|i.id==*id))
            ||loadout.enchantments.iter().any(|entry|!loadout.items.iter().any(|i|i.id.0==entry.entry.caster && matches!(i.place,bace_inventory::ItemPlace::Contained{container,equipped,..}if container==actor&&equipped!=0)) || (entry.target!=actor && entry.target.0!=entry.entry.caster))
        {return Err(GeneratorServiceError::Invalid);}
        let restored = loadout
            .combat_assets
            .as_ref()
            .is_some_and(|assets| assets.restored_registries.is_some());
        let mut ids = std::collections::BTreeSet::new();
        for item in &loadout.items {
            if item.id == actor
                || item.revision == 0
                || (!restored && item.revision != 1)
                || !ids.insert(item.id)
            {
                return Err(GeneratorServiceError::Invalid);
            }
            let ItemPlace::Contained {
                mut container,
                equipped,
                ..
            } = item.place
            else {
                return Err(GeneratorServiceError::Invalid);
            };
            if equipped != 0 && container != actor {
                return Err(GeneratorServiceError::Invalid);
            }
            let mut depth = 0;
            while container != actor {
                if depth >= 64
                    || container == item.id
                    || !loadout.containers.iter().any(|c| c.id == container)
                {
                    return Err(GeneratorServiceError::Invalid);
                }
                let parent = loadout
                    .items
                    .iter()
                    .find(|i| i.id == container)
                    .ok_or(GeneratorServiceError::Invalid)?;
                let ItemPlace::Contained {
                    container: next, ..
                } = parent.place
                else {
                    return Err(GeneratorServiceError::Invalid);
                };
                container = next;
                depth += 1;
            }
        }
        if let Some(assets) = &loadout.combat_assets
            && (assets.registry_items.len() != ids.len()
                || assets
                    .registry_items
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    != ids)
        {
            return Err(GeneratorServiceError::Invalid);
        }
        Ok(())
    }
    /// Native equipment has no durable owner until corpse/acquisition commits.
    /// This cleanup is called only after accepted destruction/corpse transfer.
    pub(in crate::kernel) fn retire_generated_creature_equipment(
        &mut self,
        actor: EntityId,
    ) -> Result<(), GeneratorServiceError> {
        self.retire_generated_creature_equipment_with_holds(actor, &[])
    }
    pub(in crate::kernel) fn retire_generated_creature_equipment_with_holds(
        &mut self,
        actor: EntityId,
        inherited: &[EntityId],
    ) -> Result<(), GeneratorServiceError> {
        if self.combat.physical_proc_pending(actor) {
            return Err(GeneratorServiceError::Busy);
        }
        if inherited.len() > 1
            || inherited.iter().any(|id| {
                *id != actor
                    || !self.npcs.deletions.values().any(|t| {
                        t.npc.context.source == actor
                            && self.world.retirement_hold(actor) == Some(t.hold)
                            && self.npcs.deletion_registries.contains(&t.npc.ticket)
                            && self.npcs.deletion_committed.contains(&t.npc.ticket)
                    })
            })
        {
            return Err(GeneratorServiceError::Busy);
        }
        let roots:Vec<_>=self.inventory.items().filter(|i|matches!(i.place,bace_inventory::ItemPlace::Contained{container,..}if container==actor)).map(|i|i.id).collect();
        let mut ids = std::collections::BTreeSet::new();
        for root in roots {
            ids.extend(
                self.inventory
                    .generated_tree(root)
                    .map_err(|_| GeneratorServiceError::Busy)?,
            );
        }
        let ids: Vec<_> = ids.into_iter().collect();
        let mut inventory = self.inventory.clone();
        if !ids.is_empty() {
            inventory
                .remove_generated_tree(&ids)
                .map_err(|_| GeneratorServiceError::Busy)?;
        }
        inventory
            .retire_generated_container(actor)
            .map_err(|_| GeneratorServiceError::Busy)?;
        let now = self.tick as f64 / 30.0;
        let mut reserved = Vec::new();
        let mut inherited_reserved = Vec::new();
        for id in ids.iter().copied().chain(std::iter::once(actor)) {
            if self.magic.registry(id).is_none() {
                continue;
            }
            if inherited.contains(&id) && self.magic.registry_reserved(id) {
                inherited_reserved.push(id);
                continue;
            }
            if self.magic.registry_reserved(id)
                || self.magic.reserve_registry(id, true, now).is_err()
            {
                for previous in reserved {
                    self.magic
                        .reserve_registry(previous, false, now)
                        .map_err(|_| GeneratorServiceError::Busy)?;
                }
                return Err(GeneratorServiceError::Busy);
            }
            reserved.push(id);
        }
        if reserved.iter().chain(inherited_reserved.iter()).any(|id| {
            if *id == actor && self.npc_combat_assets.contains_key(&actor) {
                self.magic.can_retire_npc_registry(*id, now).is_err()
            } else {
                self.magic.can_retire_item_registry(*id, now).is_err()
            }
        }) {
            for previous in reserved {
                self.magic
                    .reserve_registry(previous, false, now)
                    .map_err(|_| GeneratorServiceError::Busy)?;
            }
            return Err(GeneratorServiceError::Busy);
        }
        for id in reserved.into_iter().chain(inherited_reserved) {
            if id == actor && self.npc_combat_assets.contains_key(&actor) {
                self.magic
                    .retire_npc_registry(id, now)
                    .map_err(|_| GeneratorServiceError::Busy)?;
            } else {
                self.magic
                    .retire_item_registry(id, now)
                    .map_err(|_| GeneratorServiceError::Busy)?;
            }
            self.registry_revisions.remove(&id);
        }
        self.retire_npc_combat_assets(actor)
            .map_err(|_| GeneratorServiceError::Busy)?;
        self.cancel_generated_enchantments(actor);
        self.inventory = inventory;
        Ok(())
    }
}
