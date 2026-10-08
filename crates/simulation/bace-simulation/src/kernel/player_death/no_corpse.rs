//! Bool29 player death reserves a separate world-drop inventory proposal.
//! Ordinary CalculateDeathItems/corpse selection is never entered by ACE.
use super::*;
use crate::player_death::{
    NoCorpseDescendant, PlayerDeathTicket, PlayerNoCorpsePlan, PreparedPlayerNoCorpse,
};
use bace_entity::{EntityVital, VitalMutation};
use bace_interactions::{PlayerDeathKind, death_destination};
use bace_inventory::{InventoryView, ItemChange, ItemPlace};
use std::collections::BTreeSet;

impl Kernel {
    pub fn prepare_player_no_corpse(
        &mut self,
        prepared: PreparedPlayerNoCorpse,
    ) -> Result<(), (E, Box<PreparedPlayerNoCorpse>)> {
        let ticket = match self.check_player_no_corpse(&prepared) {
            Ok(ticket) => ticket,
            Err(error) => return Err((error, Box::new(prepared))),
        };
        let mut inventory = self.inventory.clone();
        if inventory
            .release_player_death_hold(prepared.actor, prepared.operation)
            .is_err()
        {
            return Err((E::Stale, Box::new(prepared)));
        }
        for container in &prepared.fresh_containers {
            if inventory.register_container(*container).is_err() {
                return Err((E::Invalid, Box::new(prepared)));
            }
        }
        let operation = match inventory.reserve(prepared.actor, ticket.inventory.proposal.clone()) {
            Ok(operation) => operation,
            Err(_) => return Err((E::Busy, Box::new(prepared))),
        };
        let old = std::mem::replace(&mut self.inventory, inventory);
        if self.reserve_inventory_registries(operation, &[]).is_err() {
            self.inventory = old;
            return Err((E::Busy, Box::new(prepared)));
        }
        let token = bace_world::VitalReservationToken {
            domain: bace_world::VitalReservationDomain::PlayerDeath,
            operation: prepared.operation,
        };
        if self
            .world
            .reserve_vitals(
                &[
                    (prepared.actor, EntityVital::Health),
                    (prepared.actor, EntityVital::Stamina),
                    (prepared.actor, EntityVital::Mana),
                ],
                token,
            )
            .is_err()
        {
            let _ = self.release_inventory_registries(operation);
            self.inventory = old;
            return Err((E::Busy, Box::new(prepared)));
        }
        self.inventory
            .claim(operation)
            .expect("fresh NoCorpse ticket");
        let mut ticket = ticket;
        ticket.inventory = self
            .inventory
            .pending_ticket(operation)
            .expect("reserved NoCorpse ticket")
            .clone();
        let pending = self
            .player_deaths
            .pending
            .get_mut(&prepared.actor)
            .expect("checked NoCorpse pending");
        pending.ticket = Some(ticket);
        pending.corpse = None;
        pending.world_roots = prepared.world_roots;
        pending.submitted = false;
        Ok(())
    }

    fn check_player_no_corpse(&self, p: &PreparedPlayerNoCorpse) -> Result<PlayerDeathTicket, E> {
        let pending = self.player_deaths.pending.get(&p.actor).ok_or(E::Stale)?;
        if pending.operation != p.operation
            || pending.ticket.is_some()
            || self.death_bool(p.actor, 29) != Some(true)
            || !p.animation_seconds.is_finite()
            || !(0.0..=300.).contains(&p.animation_seconds)
            || p.existing.len() + p.fresh_items.len() > 1024
            || p.world_roots.len() > 1024
        {
            return Err(E::Invalid);
        }
        let animation_ticks = ((p.animation_seconds + 1.) * 30.).ceil() as u64;
        self.tick
            .checked_add(animation_ticks)
            .and_then(|tick| tick.checked_add(90))
            .ok_or(E::Invalid)?;
        self.world
            .preflight_player_death_world_roots(p.actor, &p.world_roots)
            .map_err(|_| E::MissingAssets)?;
        let accepted = self
            .accepted_portal_position(p.actor)
            .map_err(|_| E::Invalid)?;
        let expected = bace_interactions::PortalPosition {
            cell: p.accepted_position.obj_cell_id,
            origin: [
                p.accepted_position.position_x,
                p.accepted_position.position_y,
                p.accepted_position.position_z,
            ],
            rotation: [
                p.accepted_position.rotation_w,
                p.accepted_position.rotation_x,
                p.accepted_position.rotation_y,
                p.accepted_position.rotation_z,
            ],
        };
        if accepted != expected {
            return Err(E::Stale);
        }
        let root_ids: Vec<_> = p.world_roots.iter().map(|root| root.id).collect();
        let mut unique: BTreeSet<_> = root_ids.iter().copied().collect();
        if root_ids.len() != unique.len()
            || p.existing.iter().any(|id| !unique.contains(id))
            || p.existing.iter().copied().collect::<BTreeSet<_>>().len() != p.existing.len()
        {
            return Err(E::Invalid);
        }
        let baseline: Vec<_> = self.inventory.items().cloned().collect();
        let mut parents = p
            .existing
            .iter()
            .copied()
            .filter(|id| {
                self.inventory
                    .item(*id)
                    .is_some_and(|item| item.is_container)
            })
            .collect::<Vec<_>>();
        let mut descendants = Vec::new();
        let mut descendant_ids = BTreeSet::new();
        let mut cursor = 0;
        while cursor < parents.len() {
            let parent = parents[cursor];
            let mut children = baseline
                .iter()
                .filter_map(|item| match item.place {
                    ItemPlace::Contained {
                        container,
                        slot,
                        equipped: 0,
                    } if container == parent => {
                        Some((item.pack_slot, slot, item.id, item.revision))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            children.sort_unstable();
            for (pack_slot, slot, id, revision) in children {
                if !descendant_ids.insert(id)
                    || !self.inventory.owned(p.actor, id)
                    || descendants.len() + p.existing.len() + p.fresh_items.len() + 2 > 1024
                {
                    return Err(E::Invalid);
                }
                descendants.push(NoCorpseDescendant {
                    id,
                    parent,
                    slot,
                    pack_slot,
                    revision: revision.checked_add(1).ok_or(E::Invalid)?,
                });
                if self
                    .inventory
                    .item(id)
                    .is_some_and(|item| item.is_container)
                {
                    parents.push(id);
                }
            }
            cursor += 1;
        }
        let mut changes = Vec::new();
        for &id in &p.existing {
            let before = self.inventory.item(id).ok_or(E::Stale)?.clone();
            if !self.inventory.owned(p.actor, id)
                || !matches!(before.place, ItemPlace::Contained { container, .. } if container == p.actor)
            {
                return Err(E::Stale);
            }
            let mut after = before.clone();
            after.place = ItemPlace::World;
            changes.push(ItemChange {
                before: Some(before),
                after,
            });
        }
        for descendant in &descendants {
            let before = self.inventory.item(descendant.id).ok_or(E::Stale)?.clone();
            let mut after = before.clone();
            after.revision = descendant.revision;
            changes.push(ItemChange {
                before: Some(before),
                after,
            });
        }
        let fresh_ids: BTreeSet<_> = p.fresh_items.iter().map(|item| item.id).collect();
        if fresh_ids.len() != p.fresh_items.len()
            || p.fresh_containers.iter().any(|container| {
                !fresh_ids.contains(&container.id)
                    || self.inventory.container(container.id).is_some()
                    || container.root_owner.is_some()
            })
        {
            return Err(E::Invalid);
        }
        for item in &p.fresh_items {
            if !unique.insert(item.id) && !root_ids.contains(&item.id)
                || item.revision != 0
                || self.inventory.item(item.id).is_some()
                || self.world.contains_identity(item.id)
            {
                return Err(E::Invalid);
            }
            changes.push(ItemChange {
                before: None,
                after: item.clone(),
            });
        }
        if root_ids.iter().any(|id| {
            !p.existing.contains(id) && !fresh_ids.contains(id)
                || p.fresh_items
                    .iter()
                    .any(|item| item.id == *id && item.place != ItemPlace::World)
        }) {
            return Err(E::Invalid);
        }
        let mut containers: Vec<_> = self.inventory.containers().copied().collect();
        containers.extend(p.fresh_containers.iter().copied());
        let view = InventoryView {
            items: &baseline,
            containers: &containers,
        };
        let proposal = if changes.is_empty() {
            // ACE's Olthoi/empty NoCorpse early return still changes the
            // player death checkpoint. Inventory has no item to reserve, but
            // the character death reservation and durable PlayerSaveV6 CAS
            // remain part of this exact operation.
            bace_inventory::InventoryProposal {
                changes: vec![],
                participants: vec![],
                actor_burden: view.actor_burden(p.actor).map_err(|_| E::Invalid)?,
                requires_pickup_motion: false,
            }
        } else {
            bace_inventory::propose_item_changes(p.actor, changes, view).map_err(|_| E::Invalid)?
        };
        let registry = self.magic.registry(p.actor).ok_or(E::MissingAssets)?;
        let before = self
            .player_deaths
            .states
            .get(&p.actor)
            .ok_or(E::MissingAssets)?
            .clone();
        let level = self
            .characters
            .native_services(p.actor)
            .ok_or(E::MissingAssets)?
            .level;
        let kind = self.death_kind(p.actor, pending.killer)?;
        let current_vitae = registry
            .entries()
            .iter()
            .find(|entry| entry.spell == 666)
            .map(|entry| entry.spec.value);
        let vitae = if kind == PlayerDeathKind::Pkl {
            None
        } else {
            Some(
                self.death_policy
                    .next_vitae(level, current_vitae)
                    .map_err(|_| E::Invalid)?,
            )
        };
        let purge_bad =
            kind != PlayerDeathKind::Pk && self.death_int(p.actor, 232).unwrap_or(0) > 0;
        let mut updated = self
            .magic
            .prepare_death_registry(p.actor, vitae, kind, purge_bad)
            .map_err(|_| E::MissingAssets)?;
        let lost: Vec<_> = proposal
            .changes
            .iter()
            .filter_map(|change| {
                change
                    .before
                    .as_ref()
                    .filter(|before| {
                        matches!(before.place, ItemPlace::Contained { equipped, .. } if equipped != 0)
                    })
                    .filter(|_| !matches!(change.after.place, ItemPlace::Contained { container, equipped, .. } if container == p.actor && equipped != 0))
                    .map(|before| before.id)
            })
            .collect();
        let removed: Vec<_> = updated
            .entries()
            .iter()
            .filter(|entry| entry.spell != 666 && lost.contains(&EntityId(entry.caster)))
            .map(|entry| (entry.spell, entry.spec.layer))
            .collect();
        if !removed.is_empty() {
            updated.remove(&removed).map_err(|_| E::Invalid)?;
        }
        let equipped: Vec<_> = baseline
            .iter()
            .filter(|item| self.inventory.owned(p.actor, item.id))
            .filter(
                |item| matches!(item.place, ItemPlace::Contained { equipped, .. } if equipped != 0),
            )
            .map(|item| item.id)
            .collect();
        if p.equipped_health.len() != equipped.len()
            || equipped.iter().any(|id| {
                p.equipped_health
                    .iter()
                    .filter(|(item, _)| item == id)
                    .count()
                    != 1
            })
        {
            return Err(E::MissingAssets);
        }
        let gear_health = p
            .equipped_health
            .iter()
            .filter(|(id, _)| !lost.contains(id))
            .try_fold(0u32, |sum, (_, value)| sum.checked_add(*value))
            .ok_or(E::Invalid)?;
        let maxima = self.death_vital_maxima(p.actor, &updated, p.vital_formulas, gear_health)?;
        let mut after = before.clone();
        after.num_deaths = before.num_deaths.checked_add(1).ok_or(E::Capacity)?;
        if kind != PlayerDeathKind::Pkl {
            after.death_level = level;
            after.vitae_pool = 0;
        }
        if kind != PlayerDeathKind::Ordinary {
            after.pk_status = 2;
            after.pk_respite_elapsed = Some(0.);
        }
        after.protection_elapsed = Some(0.);
        after.validate()?;
        // NoCorpse returns before Player.CalculateDeathItems and its outdoor
        // LastOutsideDeath update in the pinned source.
        let destination = death_destination(
            self.portal_links(p.actor)
                .and_then(|links| links.position(4)),
            p.instantiation,
            accepted,
        )
        .map_err(|_| E::Invalid)?;
        let destination = self
            .portal_death_teleport(p.actor, destination)
            .map_err(|_| E::MissingAssets)?;
        self.world
            .validate_teleport_batch(&[destination])
            .map_err(|_| E::MissingAssets)?;
        let vitals = [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .zip(maxima)
            .map(|(vital, maximum)| {
                let pool = self
                    .world
                    .vital(p.actor, vital)
                    .map_err(|_| E::MissingAssets)?;
                Ok(VitalMutation {
                    actor: p.actor,
                    vital,
                    before: pool.current,
                    after: bace_interactions::restored_death_vital(maximum),
                })
            })
            .collect::<Result<Vec<_>, E>>()?;
        self.world
            .validate_player_respawn(p.actor, &vitals, maxima, None)
            .map_err(|_| E::Busy)?;
        let revision = self.characters.get(p.actor).ok_or(E::Invalid)?.revision();
        Ok(PlayerDeathTicket {
            operation: p.operation,
            actor: p.actor,
            killer: pending.killer,
            kind,
            olthoi: None,
            before_revision: revision,
            after_revision: revision.checked_add(1).ok_or(E::Capacity)?,
            before,
            after,
            purge_bad,
            inventory: crate::InventoryTicket {
                operation: 0,
                actor: p.actor,
                proposal,
            },
            inventory_transcript: None,
            corpse: EntityId(0),
            no_corpse: Some(PlayerNoCorpsePlan {
                world_roots: root_ids,
                descendants,
                accepted_position: p.accepted_position.clone(),
            }),
            corpse_items: Vec::new(),
            corpse_decay_seconds: 0,
            registry_before_revision: registry.revision(),
            registry_after_revision: updated.revision(),
            before_enchantments: registry.entries().to_vec(),
            enchantments: updated.into_entries(),
            destination,
            vitals,
            post_death_maxima: maxima,
            animation_ticks,
        })
    }

    fn death_bool(&self, actor: EntityId, property: u32) -> Option<bool> {
        match self
            .world
            .properties(actor)?
            .get(bace_entity::PropertyFamily::Bool, property)?
        {
            bace_entity::PropertyValue::Bool(value) => Some(*value),
            _ => None,
        }
    }
}
