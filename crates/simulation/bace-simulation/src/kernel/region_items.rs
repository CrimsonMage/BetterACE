//! Durable world forests enter together with their region, before generators run.
use super::*;
use crate::{GeneratorServiceError, PreparedGeneratorRegion};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_magic::EnchantmentRegistry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Default, Debug)]
pub struct PreparedWorldRegionItems {
    pub items: Vec<InventoryItem>,
    pub containers: Vec<InventoryContainer>,
    pub roots: Vec<PreparedWorldRegionRoot>,
    pub registries: Vec<(EntityId, EnchantmentRegistry)>,
    pub constructed: Vec<super::constructed_creatures::PreparedRestoredConstructedCreature>,
}
#[derive(Debug)]
pub struct PreparedWorldRegionRoot {
    pub entity: EntityId,
    pub location: bace_gameplay_api::GeneratorLocation,
    pub shape: Arc<bace_physics::CollisionShape>,
    pub corpse: Option<PreparedWorldCorpse>,
}
#[derive(Debug)]
pub struct PreparedWorldCorpse {
    pub state: bace_world::CorpseState,
    /// Derived once from the persisted absolute deadline and explicit clock pair.
    pub expires_tick: u64,
    pub access: Option<crate::CorpseAccessProfile>,
}
impl PreparedWorldCorpse {
    pub fn new(
        operation: u64,
        source: EntityId,
        template: u32,
        owner: Option<EntityId>,
        expires_tick: u64,
    ) -> Self {
        Self {
            state: bace_world::CorpseState {
                operation,
                source,
                template,
                owner,
                items: Vec::new(),
            },
            expires_tick,
            access: None,
        }
    }
}
pub struct PreparedResidentRegion {
    /// Exact accepted immutable pack used to prepare every authored source.
    pub source_manifest: [u8; 32],
    /// Present only for a bounded same-epoch authored-content refresh.
    pub refresh: Option<ResidentRegionRefresh>,
    pub visibility: Vec<bace_world::PreparedCellVisibility>,
    pub region: PreparedGeneratorRegion,
    pub items: PreparedWorldRegionItems,
    pub bindings: Vec<crate::PreparedBindingObject>,
    pub dungeon: bool,
    pub keep_alive: u32,
}
pub struct ResidentRegionRefresh {
    pub expected_epoch: u64,
    pub expected_revision: u64,
    pub expected_manifest: [u8; 32],
    /// Only plain authored roots without inventory, scripts or children may be
    /// retired by this narrow transaction.
    pub remove_roots: Vec<EntityId>,
}
impl Kernel {
    pub fn admit_resident_region_complete(
        &mut self,
        mut input: PreparedResidentRegion,
    ) -> Result<(), (GeneratorServiceError, Box<PreparedResidentRegion>)> {
        let landblock = input.region.landblock;
        let epoch = input.region.epoch;
        let valid = input.refresh.is_none()
            && self
                .region_residency
                .state(landblock)
                .is_some_and(|s| s.epoch == epoch && s.phase == crate::RegionPhase::Preparing);
        if !valid
            || input.source_manifest == [0; 32]
            || self.tick.checked_add(150).is_none()
            || input.keep_alive > 4096
        {
            return Err((GeneratorServiceError::Stale, Box::new(input)));
        }
        let expected: BTreeSet<_> = input
            .region
            .geometry
            .cell_ids()
            .filter(|id| id & 0xffff >= 0x100)
            .collect();
        if input
            .visibility
            .iter()
            .map(|row| row.cell.0)
            .collect::<BTreeSet<_>>()
            != expected
            || input
                .visibility
                .iter()
                .any(|row| row.cell.0 >> 16 != u32::from(landblock))
            || self
                .world
                .validate_cell_visibility(&input.visibility)
                .is_err()
        {
            return Err((GeneratorServiceError::Invalid, Box::new(input)));
        }
        if self.preflight_binding_objects(&input.bindings).is_err()
            || input.bindings.iter().any(|binding| {
                !input
                    .region
                    .roots
                    .iter()
                    .any(|root| root.entity == binding.entity)
            })
        {
            return Err((GeneratorServiceError::Invalid, Box::new(input)));
        }
        let roots = input.region.roots.iter().map(|r| r.entity).collect();
        let plain: BTreeSet<_> = input
            .region
            .roots
            .iter()
            .filter(|r| {
                r.creature.is_none()
                    && r.loadout.is_none()
                    && r.script.is_none()
                    && !input.bindings.iter().any(|b| b.entity == r.entity)
                    && !input
                        .region
                        .definitions
                        .iter()
                        .any(|d| d.identity.entity == r.entity)
            })
            .map(|r| r.entity)
            .collect();
        if let Err(error) =
            self.admit_generator_region_inner(&input.region, &mut input.items, false)
        {
            return Err((error, Box::new(input)));
        }
        for binding in std::mem::take(&mut input.bindings) {
            self.register_binding_object(binding)
                .expect("preflighted binding root after atomic region admission");
        }
        self.world.remove_visibility_region(landblock);
        self.world
            .install_cell_visibility(input.visibility)
            .expect("same-owner visibility preflight");
        self.region_unloads.roots.insert(landblock, roots);
        self.region_plain_roots.insert(landblock, plain);
        self.region_content_heads.insert(
            landblock,
            (epoch, input.region.revision, input.source_manifest),
        );
        self.region_residency
            .admit(landblock, epoch, input.dungeon, self.tick)
            .expect("validated region epoch");
        self.region_residency
            .keep_alive(landblock, epoch, input.keep_alive)
            .expect("validated region epoch");
        Ok(())
    }
    /// Atomically add/remove simple authored objects in an occupied region.
    /// Complex roots stay on their admitted revision until their dedicated
    /// lifecycle can migrate them without losing generated or durable state.
    pub fn refresh_resident_region_content(
        &mut self,
        mut input: PreparedResidentRegion,
    ) -> Result<(), (GeneratorServiceError, Box<PreparedResidentRegion>)> {
        use GeneratorServiceError as E;
        let block = input.region.landblock;
        let Some(refresh) = input.refresh.as_ref() else {
            return Err((E::Invalid, Box::new(input)));
        };
        let expected = (
            refresh.expected_epoch,
            refresh.expected_revision,
            refresh.expected_manifest,
        );
        let state = self.region_residency.state(block);
        if self.region_content_heads.get(&block).copied() != Some(expected)
            || !state.is_some_and(|s| {
                s.epoch == refresh.expected_epoch
                    && matches!(
                        s.phase,
                        crate::RegionPhase::Active | crate::RegionPhase::Dormant
                    )
            })
            || input.region.epoch != refresh.expected_epoch
            || input.region.revision <= refresh.expected_revision
            || input.source_manifest == [0; 32]
            || input.source_manifest == refresh.expected_manifest
            || input.keep_alive > 4096
            || !input.items.items.is_empty()
            || !input.items.containers.is_empty()
            || !input.items.roots.is_empty()
            || !input.items.registries.is_empty()
            || !input.items.constructed.is_empty()
            || !input.bindings.is_empty()
            || !input.region.containers.is_empty()
            || input
                .region
                .roots
                .iter()
                .any(|r| r.creature.is_some() || r.loadout.is_some() || r.script.is_some())
            || refresh.remove_roots.len() > 4096
        {
            return Err((E::Stale, Box::new(input)));
        }
        let old_roots = self.region_unloads.roots.get(&block);
        let mut remove = BTreeSet::new();
        for &id in &refresh.remove_roots {
            if !remove.insert(id)
                || !old_roots.is_some_and(|roots| roots.contains(&id))
                || !self
                    .region_plain_roots
                    .get(&block)
                    .is_some_and(|roots| roots.contains(&id))
                || self.inventory.item(id).is_some()
                || self.npcs.has_source(id)
                || self.npcs.pending_participant(id)
                || self.world.combatant(id).is_some()
                || self.world.door(id).is_some()
                || self.world.retirement_hold(id).is_some()
                || self.world.has_reserved_vitals(id)
                || self.world.health_observation_pending_for(id)
                || !self.world.can_retire_actor_motion(id)
                || self.generators.machines.contains_key(&id)
                || self.recalls.bindings.contains_key(&id)
                || self.world.body(id).is_err()
            {
                return Err((E::Busy, Box::new(input)));
            }
        }
        if input.region.roots.iter().any(|r| {
            remove.contains(&r.entity) || old_roots.is_some_and(|roots| roots.contains(&r.entity))
        }) || input.region.roots.len() + old_roots.map_or(0, Vec::len) > 4096
            || self
                .world
                .validate_cell_visibility(&input.visibility)
                .is_err()
        {
            return Err((E::Invalid, Box::new(input)));
        }
        let mut definition_ids = BTreeSet::new();
        if input
            .region
            .definitions
            .iter()
            .any(|definition| !definition_ids.insert(definition.identity.entity))
        {
            return Err((E::Invalid, Box::new(input)));
        }
        let mut updates = Vec::new();
        let mut additions = input.region.clone();
        additions.definitions.retain(|definition| {
            let id = definition.identity.entity;
            if let Some(machine) = self.generators.machines.get(&id) {
                let mut next = machine.clone();
                if machine.definition().location.cell >> 16 == u32::from(block)
                    && old_roots.is_some_and(|roots| roots.contains(&id))
                    && next.refresh_future_content(definition.clone()).is_ok()
                {
                    updates.push((id, next));
                }
                false
            } else {
                true
            }
        });
        if updates.len() + additions.definitions.len() != input.region.definitions.len()
            || additions.definitions.iter().any(|definition| {
                !input
                    .region
                    .roots
                    .iter()
                    .any(|root| root.entity == definition.identity.entity)
            })
        {
            return Err((E::Invalid, Box::new(input)));
        }
        if let Err(error) = self.admit_generator_region_inner(&additions, &mut input.items, true) {
            return Err((error, Box::new(input)));
        }
        for (id, machine) in updates {
            self.generators.machines.insert(id, machine);
        }
        for id in &remove {
            self.world
                .remove(*id)
                .expect("preflighted plain authored root");
        }
        self.world.remove_visibility_region(block);
        self.world
            .install_cell_visibility(std::mem::take(&mut input.visibility))
            .expect("preflighted same-owner visibility");
        let roots = self
            .region_unloads
            .roots
            .get_mut(&block)
            .expect("resident roots");
        roots.retain(|id| !remove.contains(id));
        roots.extend(input.region.roots.iter().map(|r| r.entity));
        let plain = self
            .region_plain_roots
            .get_mut(&block)
            .expect("resident plain roots");
        plain.retain(|id| !remove.contains(id));
        plain.extend(input.region.roots.iter().map(|r| r.entity));
        self.region_content_heads.insert(
            block,
            (
                input.region.epoch,
                input.region.revision,
                input.source_manifest,
            ),
        );
        self.region_residency
            .keep_alive(block, input.region.epoch, input.keep_alive)
            .expect("preflighted resident keep-alive");
        Ok(())
    }
    pub(super) fn validate_world_region_items(
        &self,
        landblock: u16,
        input: &PreparedWorldRegionItems,
    ) -> Result<(), GeneratorServiceError> {
        use GeneratorServiceError as E;
        if input.items.len() > 4096
            || input.containers.len() > 4096
            || input.roots.len() > 4096
            || input.registries.len() != input.items.len()
            || input
                .registries
                .iter()
                .try_fold(0usize, |sum, (_, r)| sum.checked_add(r.entries().len()))
                .is_none_or(|n| n > 65536)
        {
            return Err(E::Capacity);
        }
        let mut ids = BTreeSet::new();
        let constructed: BTreeSet<_> = input.constructed.iter().map(|entry| entry.actor).collect();
        for item in &input.items {
            if !ids.insert(item.id)
                || self.world.contains_identity(item.id)
                || self.population.reserves_identity(item.id)
                || self.generator_reserves_identity(item.id)
                || self.physical_reserves_identity(item.id)
                || self.magic.reserves_identity(item.id)
                || !matches!(item.place, ItemPlace::World | ItemPlace::Contained { .. })
            {
                return Err(E::Stale);
            }
        }
        if input.registries.iter().any(|(id, _)| !ids.contains(id)) {
            return Err(E::Invalid);
        }
        self.magic
            .validate_region_registries(&input.registries, self.tick as f64 / 30.0)
            .map_err(|_| E::Capacity)?;
        let indexed: BTreeMap<_, _> = input.items.iter().map(|i| (i.id, i)).collect();
        for item in &input.items {
            if let ItemPlace::Contained { equipped, .. } = item.place
                && equipped != 0
            {
                let mut current = item.id;
                let mut owned_by_constructed = false;
                for _ in 0..=64 {
                    let next = indexed.get(&current).ok_or(E::Invalid)?;
                    match next.place {
                        ItemPlace::Contained { container, .. } => {
                            if constructed.contains(&container) {
                                owned_by_constructed = true;
                                break;
                            }
                            current = container;
                        }
                        _ => break,
                    }
                }
                if !owned_by_constructed {
                    return Err(E::Invalid);
                }
            }
        }
        let mut ancestry = BTreeMap::new();
        for item in &input.items {
            let mut current = item.id;
            let mut found = None;
            for _ in 0..=64 {
                let next = indexed.get(&current).ok_or(E::Invalid)?;
                match next.place {
                    ItemPlace::World => {
                        found = Some(current);
                        break;
                    }
                    ItemPlace::Contained { container, .. } => current = container,
                    ItemPlace::Removed => return Err(E::Invalid),
                }
            }
            ancestry.insert(item.id, found.ok_or(E::Invalid)?);
        }
        let mut roots = BTreeSet::new();
        let mut corpses = 0;
        let mut corpse_access = 0;
        for root in &input.roots {
            if root.location.cell >> 16 != u32::from(landblock)
                || !roots.insert(root.entity)
                || indexed
                    .get(&root.entity)
                    .is_none_or(|i| i.place != ItemPlace::World)
            {
                return Err(E::Invalid);
            }
            if let Some(corpse) = &root.corpse {
                corpses += 1;
                if let Some(profile) = &corpse.access {
                    corpse_access += 1;
                    if profile.validate().is_err()
                        || self.player_deaths.corpse_access.contains_key(&root.entity)
                    {
                        return Err(E::Invalid);
                    }
                }
                if corpse.state.operation == 0
                    || corpse.state.source.0 == 0
                    || self.corpse_expiry.deadlines.contains_key(&root.entity)
                    || !input.containers.iter().any(|c| c.id == root.entity)
                {
                    return Err(E::Invalid);
                }
                let actual: BTreeSet<_> = ancestry
                    .iter()
                    .filter_map(|(id, r)| (*r == root.entity && *id != root.entity).then_some(*id))
                    .collect();
                if actual.len() != corpse.state.items.len()
                    || actual != corpse.state.items.iter().copied().collect()
                {
                    return Err(E::Invalid);
                }
            }
        }
        if self.corpse_expiry.deadlines.len() + corpses > self.corpse_expiry.capacity {
            return Err(E::Capacity);
        }
        if self.player_deaths.corpse_access.len() + corpse_access > 4096 {
            return Err(E::Capacity);
        }
        if input
            .items
            .iter()
            .filter(|i| i.place == ItemPlace::World)
            .count()
            != roots.len()
            || input
                .containers
                .iter()
                .any(|c| c.root_owner.is_some() || !ids.contains(&c.id))
            || input
                .items
                .iter()
                .any(|i| !roots.contains(&ancestry[&i.id]))
        {
            return Err(E::Invalid);
        }
        self.validate_restored_constructed_creatures(landblock, &input.constructed, &input.items)?;
        Ok(())
    }
    pub(super) fn adopt_world_region_items(&mut self, input: &mut PreparedWorldRegionItems) {
        for (id, registry) in std::mem::take(&mut input.registries) {
            self.register_magic_registry(id, registry, true)
                .expect("registry admission preflight");
        }
        for root in std::mem::take(&mut input.roots) {
            if let Some(corpse) = root.corpse {
                self.register_corpse_expiry(
                    root.entity,
                    corpse.state.operation,
                    corpse.expires_tick,
                )
                .expect("corpse admission preflight");
                if let Some(profile) = corpse.access {
                    self.register_corpse_access(root.entity, corpse.state.operation, profile)
                        .expect("corpse access admission preflight");
                }
            }
        }
        self.adopt_restored_constructed_creatures(std::mem::take(&mut input.constructed));
        input.items.clear();
        input.containers.clear();
    }
}
#[cfg(test)]
mod tests;
