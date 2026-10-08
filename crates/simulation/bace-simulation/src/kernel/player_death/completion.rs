use super::*;
use crate::player_death::{CorpseAccessProfile, CorpseAccessState};
use crate::player_death::{PlayerDeathEvent, PlayerDeathReceipt, PlayerDeathTicket};
impl Kernel {
    pub fn take_player_death_proposal(&mut self) -> Option<PlayerDeathTicket> {
        let pending = self
            .player_deaths
            .pending
            .values_mut()
            .find(|p| p.ticket.is_some() && !p.submitted && !p.committed)?;
        pending.submitted = true;
        pending.ticket.clone()
    }
    /// Both retry and definite rollback retain the identical death selection.
    pub fn retry_player_death(&mut self, operation: u64) -> Result<(), E> {
        let p = self
            .player_deaths
            .pending
            .values_mut()
            .find(|p| p.operation == operation)
            .ok_or(E::Stale)?;
        if p.committed || !p.submitted {
            return Err(E::Stale);
        }
        p.submitted = false;
        Ok(())
    }
    pub fn confirm_player_death_committed_at(
        &mut self,
        receipt: &PlayerDeathReceipt,
        corpse_expiry_tick: u64,
    ) -> Result<(), E> {
        let p = self
            .player_deaths
            .pending
            .get(&receipt.actor)
            .ok_or(E::Stale)?;
        let t = p.ticket.as_ref().ok_or(E::Stale)?;
        if !p.submitted
            || t.operation != receipt.operation
            || t.actor != receipt.actor
            || t.after_revision != receipt.after_revision
            || t.inventory.operation != receipt.inventory.operation
            || receipt.inventory.revisions.len() != t.inventory.proposal.changes.len()
            || t.inventory.proposal.changes.iter().any(|c| {
                receipt
                    .inventory
                    .revisions
                    .iter()
                    .filter(|(id, rev)| *id == c.after.id && *rev == c.after.revision)
                    .count()
                    != 1
            })
        {
            return Err(E::Receipt);
        }
        if p.committed {
            return if p.corpse_expiry_tick == Some(corpse_expiry_tick) {
                Ok(())
            } else {
                Err(E::Receipt)
            };
        }
        if let Some(plan) = &t.no_corpse {
            if t.corpse != EntityId(0)
                || corpse_expiry_tick != 0
                || plan.world_roots.len() != p.world_roots.len()
                || plan
                    .world_roots
                    .iter()
                    .zip(&p.world_roots)
                    .any(|(id, root)| *id != root.id)
            {
                return Err(E::Receipt);
            }
            self.world
                .preflight_player_death_world_roots(receipt.actor, &p.world_roots)
                .map_err(|_| E::Busy)?;
        } else {
            self.validate_corpse_expiry_registration(t.corpse, t.operation, corpse_expiry_tick)?;
        }
        self.inventory
            .validate_receipt(&receipt.inventory)
            .map_err(|_| E::Receipt)?;
        if self.player_deaths.events.len() >= self.player_deaths.capacity {
            return Err(E::Capacity);
        }
        let registry = bace_magic::EnchantmentRegistry::restore(
            self.magic
                .registry(receipt.actor)
                .ok_or(E::MissingAssets)?
                .capacity(),
            t.registry_after_revision,
            t.enchantments.clone(),
        )
        .map_err(|_| E::Invalid)?;
        self.magic
            .validate_death_registry_silent(receipt.actor, t.registry_before_revision)
            .map_err(|_| E::Busy)?;
        let due = self.tick.checked_add(t.animation_ticks).ok_or(E::Invalid)?;
        let corpse = t.corpse;
        let start_view = p.start_view.clone();
        let num_deaths = t.after.num_deaths;
        let registry_before_revision = t.registry_before_revision;
        let penalty = t.kind != bace_interactions::PlayerDeathKind::Pkl;
        let death_level = penalty.then_some(t.after.death_level);
        let vitae_pool = penalty.then_some(t.after.vitae_pool);
        let vitae = if penalty {
            t.enchantments
                .iter()
                .find(|entry| entry.spell == 666)
                .cloned()
        } else {
            None
        };
        if penalty && vitae.is_none() {
            return Err(E::MissingAssets);
        }
        let purge_bad = t.purge_bad;
        let suicide = p.last_damager == Some(receipt.actor);
        if t.no_corpse.is_none() {
            self.register_corpse_expiry(corpse, receipt.operation, corpse_expiry_tick)?;
        }
        self.magic
            .adopt_death_registry_silent(receipt.actor, registry_before_revision, registry)
            .expect("preflighted committed death registry");
        let p = self
            .player_deaths
            .pending
            .get_mut(&receipt.actor)
            .expect("checked death");
        p.committed = true;
        p.corpse_expiry_tick = Some(corpse_expiry_tick);
        p.due = Some(due);
        self.player_deaths
            .events
            .push_back(PlayerDeathEvent::Started {
                operation: receipt.operation,
                actor: receipt.actor,
                motion: 0x40000011,
                until_tick: due,
                num_deaths,
                death_level,
                vitae_pool,
                vitae,
                purge_bad,
                suicide,
                accepted: start_view,
            });
        Ok(())
    }
    pub(in crate::kernel) fn step_player_deaths(
        &mut self,
        next_tick: u64,
    ) -> Result<(), SimulationError> {
        let mut due = [EntityId(0); 32];
        let mut count = 0;
        for (&actor, p) in &self.player_deaths.pending {
            if p.committed
                && p.due.is_some_and(|t| t <= next_tick)
                && (p.respawn_due.is_some()
                    || p.motion.is_none_or(|(token, epoch)| {
                        self.world.death_motion_complete(actor, token, epoch)
                    }))
            {
                due[count] = actor;
                count += 1;
                if count == due.len() {
                    break;
                }
            }
        }
        for actor in due.into_iter().take(count) {
            if self.player_deaths.events.len() >= self.player_deaths.capacity {
                break;
            }
            let error = self.adopt_player_death_stage(actor, next_tick).err();
            if let Some(p) = self.player_deaths.pending.get_mut(&actor) {
                p.blocked = error;
            }
        }
        self.step_player_death_timers(next_tick)?;
        Ok(())
    }
    fn adopt_player_death_stage(&mut self, actor: EntityId, now: u64) -> Result<(), E> {
        let pending = self.player_deaths.pending.get(&actor).ok_or(E::Stale)?;
        let ticket = pending.ticket.as_ref().ok_or(E::Stale)?.clone();
        let token = bace_world::VitalReservationToken {
            domain: bace_world::VitalReservationDomain::PlayerDeath,
            operation: ticket.operation,
        };
        if let Some(due) = pending.respawn_due {
            if now < due {
                return Ok(());
            }
            self.world
                .validate_player_respawn(
                    actor,
                    &ticket.vitals,
                    ticket.post_death_maxima,
                    Some(token),
                )
                .map_err(|_| E::Busy)?;
            self.world
                .apply_player_respawn(actor, &ticket.vitals, ticket.post_death_maxima, token)
                .map_err(|_| E::Busy)?;
            self.world
                .reset_player_death_history(actor)
                .map_err(|_| E::Invalid)?;
            self.world.release_vitals(token);
            self.characters
                .release_death(actor, ticket.operation)
                .map_err(|_| E::Stale)?;
            self.player_deaths.pending.remove(&actor);
            self.player_deaths
                .events
                .push_back(PlayerDeathEvent::Respawned {
                    operation: ticket.operation,
                    actor,
                    destination: bace_interactions::PortalPosition {
                        cell: ticket.destination.destination.0,
                        origin: [
                            ticket.destination.position.x,
                            ticket.destination.position.y,
                            ticket.destination.position.z,
                        ],
                        rotation: [
                            (ticket.destination.heading * 0.5).cos(),
                            0.,
                            0.,
                            (ticket.destination.heading * 0.5).sin(),
                        ],
                    },
                    accepted: self.world.accepted_object_view(actor),
                    vitals: [
                        bace_entity::EntityVital::Health,
                        bace_entity::EntityVital::Stamina,
                        bace_entity::EntityVital::Mana,
                    ]
                    .map(|vital| {
                        self.world
                            .vital(actor, vital)
                            .expect("validated respawn vital")
                            .current
                    }),
                    vital_revision: self
                        .world
                        .combatant(actor)
                        .expect("validated respawn combatant")
                        .revision(),
                });
            return Ok(());
        }
        if self
            .characters
            .get(actor)
            .is_none_or(|c| c.revision() != ticket.before_revision)
            || self.player_deaths.states.get(&actor) != Some(&ticket.before)
            || self.magic.registry(actor).is_none_or(|r| {
                r.revision() != ticket.registry_after_revision
                    || r.entries() != ticket.enchantments.as_slice()
            })
            || !self.can_admit_death_portal(actor)
        {
            return Err(E::Busy);
        }
        let receipt = crate::InventoryReceipt {
            operation: ticket.inventory.operation,
            revisions: ticket
                .inventory
                .proposal
                .changes
                .iter()
                .map(|change| (change.after.id, change.after.revision))
                .collect(),
        };
        self.inventory
            .validate_receipt(&receipt)
            .map_err(|_| E::Receipt)?;
        self.preflight_inventory_registries(receipt.operation, true)
            .map_err(|_| E::Busy)?;
        self.world
            .validate_player_respawn(actor, &ticket.vitals, ticket.post_death_maxima, Some(token))
            .map_err(|_| E::Busy)?;
        self.world
            .validate_teleport_batch(&[ticket.destination])
            .map_err(|_| E::MissingAssets)?;
        if ticket.no_corpse.is_some() {
            self.world
                .preflight_player_death_world_roots(actor, &pending.world_roots)
                .map_err(|_| E::MissingAssets)?;
        } else {
            let corpse = pending.corpse.as_ref().ok_or(E::Stale)?;
            self.world
                .validate_player_corpse(actor, corpse)
                .map_err(|_| E::MissingAssets)?;
            if self.player_deaths.corpse_access.len() >= 4096
                || self
                    .player_deaths
                    .corpse_access
                    .contains_key(&ticket.corpse)
            {
                return Err(E::Capacity);
            }
        }
        now.checked_add(90).ok_or(E::Invalid)?;
        // All fallible projection checks precede adoption. A committed receipt never
        // rolls back or rerolls when geometry/output capacity temporarily blocks it.
        self.world
            .begin_portal_transit(
                &[(actor, ticket.destination.expected_epoch)],
                ticket.operation,
            )
            .map_err(|_| E::Busy)?;
        if ticket.no_corpse.is_some() {
            let roots = std::mem::take(
                &mut self
                    .player_deaths
                    .pending
                    .get_mut(&actor)
                    .expect("checked pending")
                    .world_roots,
            );
            self.world
                .adopt_player_death_world_roots(actor, roots)
                .map_err(|_| E::Invalid)?;
        } else {
            let corpse = self
                .player_deaths
                .pending
                .get_mut(&actor)
                .expect("checked pending")
                .corpse
                .take()
                .expect("checked corpse");
            self.world
                .insert_player_corpse(
                    actor,
                    corpse,
                    bace_world::CorpseState {
                        operation: ticket.operation,
                        source: actor,
                        template: ticket
                            .inventory
                            .proposal
                            .changes
                            .iter()
                            .find(|c| c.after.id == ticket.corpse)
                            .ok_or(E::Invalid)?
                            .after
                            .template,
                        owner: Some(actor),
                        items: ticket.corpse_items.clone(),
                    },
                )
                .map_err(|_| E::Invalid)?;
            self.player_deaths.corpse_access.insert(
                ticket.corpse,
                CorpseAccessState {
                    operation: ticket.operation,
                    profile: CorpseAccessProfile {
                        victim: Some(actor),
                        killer: ticket.killer,
                        is_monster: false,
                        generated_rare: false,
                        pk_death: ticket.kind == bace_interactions::PlayerDeathKind::Pk,
                        looted: false,
                        permittees: Vec::new(),
                    },
                    viewer: None,
                },
            );
        }
        self.world
            .teleport_batch(&[ticket.destination])
            .map_err(|_| E::Busy)?;
        self.admit_death_portal(actor, ticket.operation);
        let inventory = self.inventory.confirm(&receipt).map_err(|_| E::Receipt)?;
        self.retire_inventory_registries(&inventory)
            .map_err(|_| E::Busy)?;
        self.release_inventory_registries(receipt.operation)
            .map_err(|_| E::Busy)?;
        self.registry_revisions
            .insert(actor, ticket.registry_after_revision);
        self.characters
            .adopt_death_revision(
                actor,
                ticket.operation,
                ticket.before_revision,
                ticket.after_revision,
            )
            .map_err(|_| E::Stale)?;
        self.player_deaths
            .states
            .insert(actor, ticket.after.clone());
        self.player_deaths.timers.insert(actor, now);
        self.synchronize_death_projection(actor);
        let p = self
            .player_deaths
            .pending
            .get_mut(&actor)
            .expect("pending held");
        p.respawn_due = Some(now.checked_add(90).ok_or(E::Invalid)?);
        p.due = p.respawn_due;
        if let Some(plan) = &ticket.no_corpse {
            self.player_deaths
                .events
                .push_back(PlayerDeathEvent::WorldDrops {
                    operation: ticket.operation,
                    actor,
                    roots: plan
                        .world_roots
                        .iter()
                        .map(|id| (*id, self.world.accepted_object_view(*id)))
                        .collect(),
                    accepted_player: self.world.accepted_object_view(actor),
                });
        } else {
            self.player_deaths
                .events
                .push_back(PlayerDeathEvent::Corpse {
                    operation: ticket.operation,
                    actor,
                    corpse: ticket.corpse,
                    accepted_corpse: self.world.accepted_object_view(ticket.corpse),
                    accepted_player: self.world.accepted_object_view(actor),
                });
        }
        Ok(())
    }
}
