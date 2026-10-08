//! Bounded source heartbeat work, suspended by exact valuable-operation holds.
use super::*;
use crate::equipment_mana::{EquipmentManaPlayer, EquipmentManaRecovery};
use bace_gameplay_api::social::SocialEvent;
use bace_inventory::ItemPlace;
impl Kernel {
    pub fn equipment_mana_snapshot(&self, actor: EntityId) -> Option<EquipmentManaRecovery> {
        let player = self.equipment_mana.players.get(&actor)?;
        let now = self.tick as f64 / 30.;
        Some(EquipmentManaRecovery {
            fresh: false,
            heartbeat: player.heartbeat,
            rating: player.rating,
            heartbeat_remaining: (player.next - now).min(player.heartbeat.max(5.)),
            items: player
                .items
                .values()
                .map(|item| {
                    let mut item = item.clone();
                    item.removal_remaining = player
                        .removals
                        .get(&item.item)
                        .map(|due| (due - now).clamp(0., 2.));
                    item
                })
                .collect(),
        })
    }
    pub(super) fn equipment_mana_pending(&self, actor: EntityId) -> bool {
        self.equipment_mana
            .players
            .get(&actor)
            .is_some_and(|p| !p.removals.is_empty())
    }
    pub fn register_equipment_mana(
        &mut self,
        binding: CharacterBinding,
        state: EquipmentManaRecovery,
    ) -> Result<(), &'static str> {
        let actor = binding.actor;
        state.validate(actor)?;
        let delay = if state.fresh && state.heartbeat > 0. {
            self.equipment_mana_initial_delay(binding)?
        } else {
            state.heartbeat_remaining
        };
        if self.equipment_mana.players.contains_key(&actor)
            || self.equipment_mana.players.len() >= self.equipment_mana.capacity
            || self
                .equipment_mana
                .players
                .values()
                .map(|p| p.items.len())
                .sum::<usize>()
                + state.items.len()
                > self.equipment_mana.capacity
            || self.characters.get(actor).is_none()
            || state
                .items
                .iter()
                .any(|item| !self.inventory.owned(actor, item.item))
        {
            return Err("equipment mana ownership/capacity");
        }
        let now = self.tick as f64 / 30.;
        let entry_cleanup = state.fresh
            && state
                .items
                .iter()
                .any(|item| item.removal_remaining == Some(0.));
        let removals = state
            .items
            .iter()
            .filter_map(|item| item.removal_remaining.map(|time| (item.item, now + time)))
            .collect();
        self.equipment_mana.players.insert(
            actor,
            EquipmentManaPlayer {
                entry_cleanup,
                heartbeat: state.heartbeat,
                rating: state.rating,
                next: now + delay,
                items: state
                    .items
                    .into_iter()
                    .map(|item| (item.item, item))
                    .collect(),
                removals,
                changes: Vec::new(),
                due: Vec::new(),
            },
        );
        Ok(())
    }
    pub(super) fn equipment_mana_initial_delay(
        &self,
        binding: CharacterBinding,
    ) -> Result<f64, &'static str> {
        let root = self
            .social
            .random
            .as_ref()
            .ok_or("equipment heartbeat RNG missing")?;
        let mut identity = [0u8; 16];
        identity[..8].copy_from_slice(&binding.session.0.to_le_bytes());
        identity[8..].copy_from_slice(&binding.account.0.to_le_bytes());
        let mut stream = root
            .event_stream(identity, bace_random::Domain::Magic)
            .and_then(|s| s.fork(b"equipment-heartbeat-v1", u64::from(binding.actor.0)))
            .map_err(|_| "equipment heartbeat RNG")?;
        Ok(
            (stream.next_u64().map_err(|_| "equipment heartbeat draw")? >> 11) as f64
                * (5. / ((1u64 << 53) as f64)),
        )
    }
    pub(super) fn step_equipment_mana(&mut self, now: f64) -> Result<(), SimulationError> {
        let mut ids = std::mem::take(&mut self.equipment_mana.scratch);
        ids.clear();
        for (&actor, player) in &mut self.equipment_mana.players {
            player.items.retain(|id, _| {
                player.removals.contains_key(id) || self.inventory.owned(actor, *id)
            });
            if (player.heartbeat > 0. && player.next <= now)
                || player.removals.values().any(|due| *due <= now)
            {
                ids.push(actor);
            }
        }
        for actor in &ids {
            let mut player = self
                .equipment_mana
                .players
                .remove(actor)
                .expect("known mana owner");
            let result = self.step_equipment_mana_actor(*actor, &mut player, now);
            self.equipment_mana.players.insert(*actor, player);
            if result.is_err() {
                self.equipment_mana.scratch = ids;
                return result;
            }
        }
        ids.clear();
        self.equipment_mana.scratch = ids;
        Ok(())
    }
    fn step_equipment_mana_actor(
        &mut self,
        actor: EntityId,
        player: &mut EquipmentManaPlayer,
        now: f64,
    ) -> Result<(), SimulationError> {
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
            || self.housing.reserved(actor)
            || self.portals.reserved(actor)
            || self.magic.registry_reserved(actor)
            || self.world.has_reserved_vitals(actor)
        {
            return Ok(());
        }
        // A due removal is a retained owner obligation. It precedes later burns.
        player.due.clear();
        player.due.extend(
            player
                .removals
                .iter()
                .filter(|(_, due)| **due <= now)
                .map(|(id, _)| *id),
        );
        for &id in &player.due {
            if self.inventory.reserved(id) || self.magic.registry_reserved(id) {
                continue;
            }
            let item = player
                .items
                .get(&id)
                .ok_or(SimulationError::InvalidCommand)?;
            let patches = prepare_depletion(&self.magic, actor, item, !player.entry_cleanup)?;
            let mut held = Vec::new();
            let result = (|| {
                for patch in &patches {
                    self.magic.reserve_registry(patch.actor, true, now)?;
                    held.push(patch.actor);
                }
                self.magic.validate_item_experience_registries(&patches)?;
                self.magic.adopt_item_experience_registries(&patches);
                Ok::<(), bace_gameplay_api::CastRejection>(())
            })();
            for target in held {
                self.magic
                    .reserve_registry(target, false, now)
                    .map_err(|_| SimulationError::InvalidCommand)?;
            }
            if result.is_err() {
                continue;
            }
            player.removals.remove(&id);
            player
                .items
                .get_mut(&id)
                .expect("known mana item")
                .removal_remaining = None;
        }
        if player.removals.is_empty() {
            player.entry_cleanup = false;
        }
        if player.heartbeat <= 0. || now < player.next {
            return Ok(());
        }
        // Prepare one complete heartbeat before mutating, preserving exact old
        // state under output pressure or any held item. No heartbeat is dropped.
        player
            .items
            .retain(|id, _| self.inventory.owned(actor, *id));
        player.changes.clear();
        let mut notices = 0;
        for (&id, item) in &player.items {
            let Some(inventory) = self.inventory.item(id) else {
                return Err(SimulationError::InvalidCommand);
            };
            if !matches!(inventory.place,ItemPlace::Contained{container,equipped,..} if container==actor && equipped!=0)
            {
                continue;
            }
            if self.inventory.reserved(id) || self.magic.registry_reserved(id) {
                return Ok(());
            }
            let (after, notice) =
                crate::equipment_mana::heartbeat(item, player.heartbeat, player.rating)
                    .map_err(|_| SimulationError::InvalidCommand)?;
            if (after.current != item.current || after.affecting != item.affecting)
                && inventory.revision == u64::MAX
            {
                return Err(SimulationError::AuxiliaryRevision);
            }
            notices += usize::from(notice.is_some());
            player.changes.push((id, after, notice));
        }
        if notices > self.social.capacity - self.social.events.len() {
            return Ok(());
        }
        for &(id, after, notice) in &player.changes {
            let before = &player.items[&id];
            if after.current != before.current || after.affecting != before.affecting {
                let revision = self.inventory.item(id).expect("mana preflight").revision;
                self.inventory
                    .touch_registry(id)
                    .map_err(|_| SimulationError::AuxiliaryRevision)?;
                match self
                    .combat
                    .rebind_physical_registry_revision(id, revision, revision + 1)
                {
                    Ok(()) | Err(crate::SkillRefreshError::Busy) => {}
                    Err(_) => return Err(SimulationError::SkillRefresh),
                }
            }
            if let Some(depleted) = notice {
                self.social.events.push_back(SocialEvent::EquipmentMana {
                    recipient: actor,
                    name: before.name.clone(),
                    depleted,
                });
                if depleted {
                    player.removals.insert(id, now + 2.);
                }
            }
            after.apply(player.items.get_mut(&id).expect("prepared mana item"));
        }
        self.queue_gag_heartbeat(actor, player.heartbeat);
        player.next = now + player.heartbeat;
        Ok(())
    }
}

/// Clone each affected registry once; source spell-book order chooses the first
/// matching caster entry. Bounded batches avoid rebuilding a 4096-entry registry
/// for each of an item's possible 256 known spells on the simulation thread.
fn prepare_depletion(
    magic: &crate::magic::Magic,
    recipient: EntityId,
    item: &crate::EquipmentManaItem,
    notify: bool,
) -> Result<Vec<crate::ItemExperienceRegistryChange>, SimulationError> {
    let mut selected: Vec<(EntityId, Vec<(u32, u16)>)> = Vec::new();
    for &(target, spell) in &item.removals {
        let registry = magic
            .registry(target)
            .ok_or(SimulationError::InvalidCommand)?;
        if let Some(entry) = registry
            .entries()
            .iter()
            .find(|e| e.spell == spell && e.caster == item.item.0)
        {
            let key = (entry.spell, entry.spec.layer);
            if let Some((_, keys)) = selected.iter_mut().find(|(id, _)| *id == target) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            } else {
                selected.push((target, vec![key]));
            }
        }
    }
    let mut patches = Vec::with_capacity(selected.len());
    for (actor, keys) in selected {
        let before = magic
            .registry(actor)
            .ok_or(SimulationError::InvalidCommand)?;
        let mut after = bace_magic::EnchantmentRegistry::restore(
            before.capacity(),
            before.revision(),
            before.entries().to_vec(),
        )
        .map_err(|_| SimulationError::InvalidCommand)?;
        after
            .remove(&keys)
            .map_err(|_| SimulationError::AuxiliaryRevision)?;
        patches.push(crate::ItemExperienceRegistryChange {
            actor,
            capacity: before.capacity(),
            before_revision: before.revision(),
            before: before.entries().to_vec(),
            after_revision: after.revision(),
            after: after.into_entries(),
            events: keys
                .into_iter()
                .filter(|_| notify)
                .map(|key| crate::MagicEvent::EnchantmentExpired {
                    actor,
                    recipient,
                    spell: key.0,
                    layer: if key.0 == 666 { 0 } else { key.1 },
                    item_name: (actor != recipient).then(|| item.name.clone()),
                    sound: actor != recipient
                        || before
                            .entries()
                            .iter()
                            .find(|e| e.spell == key.0 && e.spec.layer == key.1)
                            .is_some_and(|e| e.spec.category != 0x8000),
                })
                .collect(),
        });
    }
    Ok(patches)
}

#[cfg(test)]
mod tests;
