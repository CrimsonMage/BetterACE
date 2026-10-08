//! Source unload lifecycle followed by exact save-before-eviction. Pending cold
//! work and any blocked owner remain attached to the draining epoch.
use super::*;
use crate::GeneratorControl;
use crate::ResidencyError as E;
use crate::region_unload::*;
impl Kernel {
    pub fn has_region_unload_state(&self) -> bool {
        !self.region_unloads.pending.is_empty() || !self.region_unloads.roots.is_empty()
    }
    pub fn peek_region_unload_proposal(&self) -> Option<&RegionUnloadTicket> {
        self.region_unloads
            .pending
            .values()
            .find(|p| p.ticket.is_some() && !p.submitted && !p.saved)?
            .ticket
            .as_ref()
    }
    pub fn take_region_unload_proposal(&mut self) -> Option<RegionUnloadTicket> {
        let p = self
            .region_unloads
            .pending
            .values_mut()
            .find(|p| p.ticket.is_some() && !p.submitted && !p.saved)?;
        p.submitted = true;
        p.ticket.clone()
    }
    pub fn retry_region_unload(&mut self, operation: u64) -> Result<(), E> {
        let p = self
            .region_unloads
            .pending
            .values_mut()
            .find(|p| p.ticket.as_ref().is_some_and(|t| t.operation == operation))
            .ok_or(E::Stale)?;
        if p.saved || !p.submitted {
            return Err(E::Stale);
        }
        p.submitted = false;
        Ok(())
    }
    pub fn region_unload_blocked(&self, landblock: u16) -> Option<E> {
        self.region_unloads
            .pending
            .get(&landblock)
            .and_then(|p| p.blocked)
    }
    pub fn confirm_region_unload_saved(&mut self, receipt: &RegionUnloadReceipt) -> Result<(), E> {
        let p = self
            .region_unloads
            .pending
            .get_mut(&receipt.landblock)
            .ok_or(E::Stale)?;
        let t = p.ticket.as_ref().ok_or(E::Stale)?;
        if !p.submitted
            || p.epoch != receipt.epoch
            || t.operation != receipt.operation
            || t.items.len() != receipt.revisions.len()
            || t.items.iter().any(|i| {
                receipt
                    .revisions
                    .iter()
                    .filter(|(id, revision, registry)| {
                        *id == i.item.id
                            && *revision == i.item.revision
                            && *registry == i.registry_revision
                    })
                    .count()
                    != 1
            })
        {
            return Err(E::Stale);
        }
        p.saved = true;
        Ok(())
    }
    pub(in crate::kernel) fn step_region_unloads(&mut self) {
        let mut blocks = [0u16; 32];
        let mut count = 0;
        for wrapped in [false, true] {
            for (block, state) in self.region_residency.states() {
                if self.region_unloads.cursor.is_some_and(|last| block <= last) != wrapped {
                    continue;
                }
                if state.phase == crate::RegionPhase::Draining {
                    blocks[count] = block;
                    count += 1;
                    if count == blocks.len() {
                        break;
                    }
                }
            }
            if count == blocks.len() {
                break;
            }
        }
        if count > 0 {
            self.region_unloads.cursor = Some(blocks[count - 1]);
        }
        for block in blocks.into_iter().take(count) {
            let error = self.advance_region_unload(block).err();
            if let Some(p) = self.region_unloads.pending.get_mut(&block) {
                p.blocked = error;
            }
        }
    }
    fn advance_region_unload(&mut self, block: u16) -> Result<(), E> {
        let state = self.region_residency.state(block).ok_or(E::Stale)?;
        if state.phase != crate::RegionPhase::Draining {
            return Err(E::Stale);
        }
        if self.combat.references_region(block, &self.world) {
            return Err(E::Busy);
        }
        if self.world.states().any(|(id, cell, _)| {
            cell.0 >> 16 == u32::from(block) && self.characters.get(id).is_some()
        }) {
            return Err(E::Busy);
        }
        self.region_unloads
            .pending
            .entry(block)
            .or_insert_with(|| PendingRegionUnload {
                epoch: state.epoch,
                notified: Default::default(),
                ticket: None,
                submitted: false,
                saved: false,
                blocked: None,
            });
        if self.region_unloads.pending[&block].epoch != state.epoch {
            return Err(E::Stale);
        }
        if self.region_unloads.pending[&block].ticket.is_none() {
            self.drain_region_generators(block)?;
            self.capture_region_unload(block, state.epoch)?;
        }
        if !self.region_unloads.pending[&block].saved {
            return Ok(());
        }
        self.evict_saved_region(block)
    }
    fn drain_region_generators(&mut self, block: u16) -> Result<(), E> {
        if self.world.states().any(|(actor, cell, _)| {
            cell.0 >> 16 == u32::from(block) && self.vendor_lazy_pending(actor)
        }) {
            return Err(E::Busy);
        }
        if !self.magic.retire_region_projectiles(&mut self.world, block)
            || !self
                .combat
                .retire_region_projectiles(&mut self.world, block)
        {
            return Err(E::Busy);
        }
        let ids: Vec<_> = self
            .generators
            .machines
            .values()
            .filter(|m| m.definition().location.cell >> 16 == u32::from(block))
            .map(|m| m.definition().identity)
            .collect();
        for identity in ids {
            if self.region_unloads.pending[&block]
                .notified
                .contains(&identity.entity)
            {
                continue;
            }
            self.generator_control(identity, GeneratorControl::Unload)
                .map_err(|_| E::Busy)?;
            self.region_unloads
                .pending
                .get_mut(&block)
                .expect("pending")
                .notified
                .insert(identity.entity);
        }
        // Effects may address children in neighbouring blocks. Preserve the
        // existing bounded depth-first lifecycle owner until every effect drains.
        self.process_generator_lifecycle().map_err(|_| E::Busy)?;
        if !self.generators.effects.is_empty() || !self.generators.lifecycle_frames.is_empty() {
            return Err(E::Busy);
        }
        // Generator unload has now withdrawn transient contributions. Do not
        // discard a vendor owner while any stock still awaits durable handoff.
        if self.world.states().any(|(actor, cell, _)| {
            cell.0 >> 16 == u32::from(block)
                && (self.vendor_lazy_pending(actor)
                    || self.vendor_stock_nonempty(actor) && !self.vendor_stock_durable(actor))
        }) {
            return Err(E::Busy);
        }
        if self.constructed_creatures.transient_in_region(block) {
            // A transient construction has no durable aggregate to restore.
            return Err(E::Busy);
        }
        let bindings: Vec<_> = self
            .recalls
            .bindings
            .keys()
            .copied()
            .filter(|&id| {
                self.world
                    .actor_state(id)
                    .is_ok_and(|(cell, _)| cell.0 >> 16 == u32::from(block))
            })
            .collect();
        self.unregister_binding_objects(&bindings)
            .map_err(|_| E::Busy)?;
        let actors: Vec<_> = self
            .world
            .states()
            .filter(|(_, cell, _)| cell.0 >> 16 == u32::from(block))
            .map(|(id, _, _)| id)
            .collect();
        for actor in actors {
            if self.inventory.item(actor).is_some() {
                continue;
            }
            if self.combat.physical_proc_pending(actor)
                || !self.npcs.can_retire_idle_source(actor)
                || self.world.health_observation_pending_for(actor)
                || self.world.has_reserved_vitals(actor)
                || !self.world.can_retire_actor_motion(actor)
            {
                return Err(E::Busy);
            }
            if let Some(origin) = self.population.generated_origin(actor) {
                self.population
                    .can_remove_generated(actor, origin)
                    .map_err(|_| E::Busy)?;
                self.retire_generated_creature_equipment(actor)
                    .map_err(|_| E::Busy)?;
                self.population
                    .remove_generated(actor, origin, &mut self.world)
                    .map_err(|_| E::Busy)?;
            } else if self
                .region_unloads
                .roots
                .get(&block)
                .is_some_and(|roots| roots.contains(&actor))
            {
                self.retire_generated_creature_equipment(actor)
                    .map_err(|_| E::Busy)?;
                self.world.remove(actor).ok_or(E::Busy)?;
            } else {
                return Err(E::Busy);
            }
            self.combat.retire_actor(actor);
            self.npcs
                .retire_idle_source(actor)
                .expect("preflighted idle script");
            self.generated_vendors.remove(&actor);
        }
        Ok(())
    }
    fn capture_region_unload(&mut self, block: u16, epoch: u64) -> Result<(), E> {
        let roots: Vec<_> = self
            .inventory
            .items()
            .filter(|i| {
                i.place == bace_inventory::ItemPlace::World
                    && self
                        .world
                        .actor_state(i.id)
                        .is_ok_and(|(cell, _)| cell.0 >> 16 == u32::from(block))
            })
            .map(|i| i.id)
            .collect();
        let mut saved = std::collections::BTreeSet::new();
        for root in roots {
            let ids = self.inventory.world_tree(root).map_err(|_| E::Busy)?;
            if ids.iter().all(|&id| self.inventory.item_transient(id)) {
                let ids = self.inventory.generated_tree(root).map_err(|_| E::Busy)?;
                self.retire_transient_region_tree(&ids)?;
            } else {
                saved.extend(ids);
            }
        }
        if saved.len() > 4096 {
            return Err(E::Capacity);
        }
        if saved
            .iter()
            .any(|&id| !self.npcs.can_retire_idle_source(id))
        {
            return Err(E::Busy);
        }
        self.preflight_restored_constructed_unload(block, &saved)
            .map_err(|_| E::Busy)?;
        let operation = self.region_unloads.next.checked_add(1).ok_or(E::Capacity)?;
        let now = self.tick as f64 / 30.;
        let mut held = Vec::new();
        for &id in &saved {
            if self.inventory.reserved(id) {
                self.release_unload_registry_holds(&held, now);
                return Err(E::Busy);
            }
            if self.magic.registry(id).is_none() {
                continue;
            }
            if self.magic.registry_reserved(id)
                || self.magic.reserve_registry(id, true, now).is_err()
            {
                self.release_unload_registry_holds(&held, now);
                return Err(E::Busy);
            }
            held.push(id);
        }
        if self.sync_registry_revisions().is_err() {
            self.release_unload_registry_holds(&held, now);
            return Err(E::Busy);
        }
        if held
            .iter()
            .try_fold(0usize, |sum, id| {
                sum.checked_add(self.magic.registry(*id).map_or(0, |r| r.entries().len()))
            })
            .is_none_or(|n| n > 65536)
        {
            self.release_unload_registry_holds(&held, now);
            return Err(E::Capacity);
        }
        let ids: Vec<_> = saved.into_iter().collect();
        if self.inventory.hold_region_items(&ids, operation).is_err() {
            self.release_unload_registry_holds(&held, now);
            return Err(E::Busy);
        }
        let items = ids
            .iter()
            .map(|&id| {
                let item = self.inventory.item(id).ok_or(E::Stale)?.clone();
                let position = if item.place == bace_inventory::ItemPlace::World {
                    Some(self.accepted_portal_position(id).map_err(|_| E::Invalid)?)
                } else {
                    None
                };
                let registry = self.magic.registry(id);
                Ok(RegionUnloadItem {
                    transient: self.inventory.item_transient(id),
                    item,
                    container: self.inventory.container(id).copied(),
                    registry_revision: registry.map(|r| r.revision()),
                    enchantments: registry.map_or_else(Vec::new, |r| r.entries().to_vec()),
                    position,
                    corpse: self.world.corpse(id).cloned(),
                })
            })
            .collect::<Result<Vec<_>, E>>();
        let items = match items {
            Ok(v) => v,
            Err(e) => {
                self.inventory.release_region_items(operation);
                self.release_unload_registry_holds(&held, now);
                return Err(e);
            }
        };
        self.region_unloads.next = operation;
        let p = self
            .region_unloads
            .pending
            .get_mut(&block)
            .expect("pending");
        p.saved = items.is_empty();
        p.ticket = Some(RegionUnloadTicket {
            operation,
            landblock: block,
            epoch,
            items,
        });
        Ok(())
    }
    fn release_unload_registry_holds(&mut self, ids: &[EntityId], now: f64) {
        for &id in ids {
            self.magic
                .reserve_registry(id, false, now)
                .expect("same-tick acquired registry hold");
        }
    }
    fn retire_transient_region_tree(&mut self, ids: &[EntityId]) -> Result<(), E> {
        self.inventory
            .preflight_transient_region_tree(ids)
            .map_err(|_| E::Busy)?;
        let now = self.tick as f64 / 30.;
        let mut held = Vec::new();
        for &id in ids {
            if self.magic.registry(id).is_none() {
                continue;
            }
            if self.magic.registry_reserved(id)
                || self.magic.reserve_registry(id, true, now).is_err()
            {
                self.release_unload_registry_holds(&held, now);
                return Err(E::Busy);
            }
            held.push(id);
        }
        if held
            .iter()
            .any(|&id| self.magic.can_retire_item_registry(id, now).is_err())
        {
            self.release_unload_registry_holds(&held, now);
            return Err(E::Busy);
        }
        if ids.iter().any(|&id| {
            !self.npcs.can_retire_idle_source(id)
                || self.world.health_observation_pending_for(id)
                || self.world.has_reserved_vitals(id)
                || !self.world.can_retire_actor_motion(id)
        }) {
            self.release_unload_registry_holds(&held, now);
            return Err(E::Busy);
        }
        for id in held {
            self.magic
                .retire_item_registry(id, now)
                .expect("preflighted registry");
            self.registry_revisions.remove(&id);
        }
        for &id in ids {
            self.world.remove(id);
            self.npcs
                .retire_idle_source(id)
                .expect("preflighted idle script");
        }
        self.inventory.adopt_transient_region_tree(ids);
        Ok(())
    }
    fn evict_saved_region(&mut self, block: u16) -> Result<(), E> {
        if !self.region_residency.can_confirm_unloaded() {
            return Err(E::Capacity);
        }
        let t = self.region_unloads.pending[&block]
            .ticket
            .as_ref()
            .ok_or(E::Stale)?;
        let now = self.tick as f64 / 30.;
        for i in &t.items {
            if !self.npcs.can_retire_idle_source(i.item.id) {
                return Err(E::Busy);
            }
            if self.corpse_is_open(i.item.id) {
                return Err(E::Busy);
            }
            if self.inventory.item(i.item.id) != Some(&i.item)
                || self.magic.registry(i.item.id).map(|r| r.revision()) != i.registry_revision
            {
                return Err(E::Stale);
            }
            if i.registry_revision.is_some() {
                self.magic
                    .can_retire_item_registry(i.item.id, now)
                    .map_err(|_| E::Busy)?;
            }
            if self.world.has_reserved_vitals(i.item.id)
                || self.world.health_observation_pending_for(i.item.id)
                || !self.world.can_retire_actor_motion(i.item.id)
            {
                return Err(E::Busy);
            }
        }
        // Projectiles have a separate owner. Geometry eviction refuses to proceed
        // until that owner has emitted and retained its removal projections.
        if self.world.region_has_projectiles(block)
            || self.combat.references_region(block, &self.world)
        {
            return Err(E::Busy);
        }
        let revisions: Vec<_> = t
            .items
            .iter()
            .map(|i| (i.item.id, i.item.revision))
            .collect();
        let removed: Vec<_> = revisions.iter().map(|(id, _)| *id).collect();
        if !self.world.can_evict_region_after(block, &removed)
            || !self.doors.can_retire_region(block)
        {
            return Err(E::Busy);
        }
        self.inventory
            .evict_region_items(&revisions, t.operation)
            .map_err(|_| E::Stale)?;
        self.retire_restored_constructed_region(block);
        let t = self
            .region_unloads
            .pending
            .get_mut(&block)
            .expect("pending region")
            .ticket
            .take()
            .expect("preflighted save ticket");
        for i in t.items {
            if i.registry_revision.is_some() {
                self.magic
                    .retire_item_registry(i.item.id, now)
                    .expect("preflighted saved registry");
                self.registry_revisions.remove(&i.item.id);
            }
            self.world.remove(i.item.id);
            self.npcs
                .retire_idle_source(i.item.id)
                .expect("preflighted idle script");
            self.corpse_expiry.deadlines.remove(&i.item.id);
            self.corpse_expiry.blocked.remove(&i.item.id);
            if let Some(corpse) = i.corpse
                && self.player_deaths.corpse_access.contains_key(&i.item.id)
            {
                self.forget_corpse_access(i.item.id, corpse.operation)
                    .map_err(|_| E::Stale)?;
            }
        }
        self.doors.retire_region(block).map_err(|_| E::Busy)?;
        self.world.evict_empty_region(block).map_err(|_| E::Busy)?;
        self.generators
            .machines
            .retain(|_, m| m.definition().location.cell >> 16 != u32::from(block));
        self.generators
            .cursors
            .retain(|id, _| self.generators.machines.contains_key(id));
        self.region_unloads.roots.remove(&block);
        self.region_content_heads.remove(&block);
        self.region_plain_roots.remove(&block);
        self.region_unloads.pending.remove(&block);
        self.region_residency.confirm_unloaded(block, t.epoch)
    }
}
#[cfg(test)]
mod tests;
