use super::*;
use crate::player_death::{
    PendingDeath, PlayerDeathAnnouncement, PlayerDeathEvent, PlayerDeathTicket, PreparedPlayerDeath,
};
use bace_entity::{EntityVital, VitalMutation};
use bace_interactions::{PlayerDeathKind, death_destination};
impl Kernel {
    /// Called from the accepted combat event lane. Output pressure keeps that event queued.
    pub fn begin_player_death(
        &mut self,
        actor: EntityId,
        last_killer: Option<EntityId>,
    ) -> Result<(), E> {
        self.begin_player_death_with_blow(actor, last_killer, None)
    }
    pub fn begin_player_death_with_blow(
        &mut self,
        actor: EntityId,
        last_damager: Option<EntityId>,
        death_blow: Option<crate::DeathBlow>,
    ) -> Result<(), E> {
        if self.combat.physical_proc_pending(actor) {
            return Err(E::Busy);
        }
        if self.player_deaths.pending.contains_key(&actor) {
            return Ok(());
        }
        if self.player_deaths.pending.len() >= self.player_deaths.capacity
            || self.player_deaths.events.len() >= self.player_deaths.capacity
        {
            return Err(E::Capacity);
        }
        if !self.player_deaths.states.contains_key(&actor) || self.player_deaths.random.is_none() {
            return Err(E::MissingAssets);
        }
        if self.world.combatant(actor).is_none_or(|c| c.health() != 0)
            || self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
            || self.housing.reserved(actor)
            || self.portals.reserved(actor)
        {
            return Err(E::Busy);
        }
        self.magic
            .cancel_for_player_death(actor, &mut self.world)
            .map_err(|_| E::Busy)?;
        self.magic
            .prepare_registry_time(self.magic.registry_time())
            .map_err(|_| E::Busy)?;
        self.sync_registry_revisions().map_err(|_| E::Invalid)?;
        let operation = self.player_deaths.next.checked_add(1).ok_or(E::Capacity)?;
        let announcement = self.death_announcement(actor, last_damager, death_blow, operation)?;
        let motion = if self.world.has_death_motions(actor) {
            let epoch = self
                .world
                .actor_state(actor)
                .map_err(|_| E::MissingAssets)?
                .1
                .epoch();
            Some((
                self.world.begin_death_motion(actor).map_err(|_| E::Busy)?,
                epoch,
            ))
        } else {
            None
        }; // Existing synthetic owner fixtures have no DAT program.
        let start_view = self.world.accepted_object_view(actor);
        let before_revision = self
            .characters
            .get(actor)
            .ok_or(E::MissingAssets)?
            .revision();
        let killer = self
            .world
            .combatant(actor)
            .and_then(|c| {
                c.contributors()
                    .iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|v| v.0)
            })
            .or(last_damager);
        let olthoi = self.olthoi_death_kind(actor, killer);
        let killer_is_olthoi =
            killer.is_some_and(|id| self.social_presence(id).is_some_and(|p| p.olthoi));
        let corpse_killer = killer.filter(|id| *id != actor).map(|id| {
            let properties = self.world.properties(id);
            let name = match properties.and_then(|p| p.get(bace_entity::PropertyFamily::String, 1))
            {
                Some(bace_entity::PropertyValue::String(name)) => name.clone(),
                _ => String::new(),
            };
            (self.pet_owner(id).unwrap_or(id), name)
        });
        self.world
            .stop_player_death_motion(actor)
            .map_err(|_| E::Invalid)?;
        self.inventory
            .hold_player_death(actor, operation)
            .map_err(|_| E::Busy)?;
        if self.characters.reserve_death(actor, operation).is_err() {
            self.inventory
                .release_player_death_hold(actor, operation)
                .expect("fresh death hold");
            return Err(E::Busy);
        }
        self.combat.cancel(actor);
        // Pinned GDLE Player::OnDeath clears PK activity on the victim.
        self.world
            .combatant_mut(actor)
            .expect("validated dead player")
            .clear_pk_activity();
        self.recalls.pending.remove(&actor);
        self.recalls.pending_bindings.remove(&actor);
        self.player_deaths.next = operation;
        self.player_deaths.pending.insert(
            actor,
            PendingDeath {
                motion,
                start_view,
                operation,
                killer,
                last_damager,
                submitted: false,
                ticket: None,
                corpse: None,
                world_roots: Vec::new(),
                committed: false,
                due: None,
                respawn_due: None,
                blocked: None,
                corpse_expiry_tick: None,
            },
        );
        self.player_deaths
            .events
            .push_back(PlayerDeathEvent::Prepare {
                operation,
                actor,
                before_revision,
                killer,
                killer_is_olthoi,
                corpse_killer,
                olthoi,
                announcement,
            });
        Ok(())
    }
    pub(super) fn death_announcement(
        &self,
        actor: EntityId,
        last_damager: Option<EntityId>,
        death_blow: Option<crate::DeathBlow>,
        operation: u64,
    ) -> Result<Option<PlayerDeathAnnouncement>, E> {
        let Some(blow) = death_blow else {
            return Ok(None);
        };
        let name = |id| match self
            .world
            .properties(id)
            .and_then(|p| p.get(bace_entity::PropertyFamily::String, 1))
        {
            Some(bace_entity::PropertyValue::String(value))
                if !value.is_empty() && value.len() <= 128 =>
            {
                Some(value.as_str())
            }
            _ => None,
        };
        let Some(victim) = name(actor) else {
            return Ok(None);
        };
        let damager = match last_damager {
            Some(id) if id != actor => match name(id) {
                Some(value) => value,
                None => return Ok(None),
            },
            _ => "",
        };
        let template = if damager.is_empty() {
            bace_combat::death_messages::unattended_death_template()
        } else if blow.critical && last_damager.is_some_and(|id| self.characters.get(id).is_some())
        {
            bace_combat::death_messages::pk_critical_killer_template()
        } else {
            let templates = bace_combat::death_messages::death_message_templates(
                blow.damage_type,
                blow.critical,
            );
            let root = self.player_deaths.random.as_ref().ok_or(E::MissingAssets)?;
            let mut event = [0; 16];
            event[..8].copy_from_slice(&self.player_deaths.epoch.to_le_bytes());
            event[8..].copy_from_slice(&operation.to_le_bytes());
            let mut stream = root
                .event_stream(event, bace_random::Domain::PlayerDeath)
                .and_then(|r| r.fork(b"death-announcement", u64::from(actor.0)))
                .map_err(|_| E::Invalid)?;
            let index = stream
                .below(templates.len() as u64)
                .map_err(|_| E::Invalid)? as usize;
            templates[index]
        };
        let format = bace_combat::death_messages::format_death_message;
        Ok(Some(PlayerDeathAnnouncement {
            last_damager,
            victim_text: format(template.victim, victim, damager),
            killer_text: (!damager.is_empty()).then(|| format(template.killer, victim, damager)),
            broadcast_text: format(template.broadcast, victim, damager),
        }))
    }
    pub fn prepare_player_death(
        &mut self,
        prepared: PreparedPlayerDeath,
    ) -> Result<(), (E, Box<PreparedPlayerDeath>)> {
        match self.check_player_death(&prepared) {
            Err(error) => Err((error, Box::new(prepared))),
            Ok(ticket) => {
                // Inventory reservation starts from a clone so failed admission is atomic.
                let mut inventory = self.inventory.clone();
                if inventory
                    .release_player_death_hold(prepared.actor, prepared.operation)
                    .is_err()
                {
                    return Err((E::Stale, Box::new(prepared)));
                }
                if inventory
                    .register_container(prepared.corpse_container)
                    .is_err()
                {
                    return Err((E::Invalid, Box::new(prepared)));
                }
                let operation =
                    match inventory.reserve(prepared.actor, ticket.inventory.proposal.clone()) {
                        Ok(op) => op,
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
                self.inventory.claim(operation).expect("fresh death ticket");
                let mut ticket = ticket;
                ticket.inventory = self
                    .inventory
                    .pending_ticket(operation)
                    .expect("reserved ticket")
                    .clone();
                let pending = self
                    .player_deaths
                    .pending
                    .get_mut(&prepared.actor)
                    .expect("checked pending");
                pending.ticket = Some(ticket);
                pending.corpse = Some(prepared.corpse);
                pending.submitted = false;
                Ok(())
            }
        }
    }
    fn check_player_death(&self, p: &PreparedPlayerDeath) -> Result<PlayerDeathTicket, E> {
        let pending = self.player_deaths.pending.get(&p.actor).ok_or(E::Stale)?;
        if pending.operation != p.operation || pending.ticket.is_some() {
            return Err(E::Stale);
        }
        if !p.animation_seconds.is_finite() || !(0.0..=300.).contains(&p.animation_seconds) {
            return Err(E::MissingAssets);
        }
        let animation_ticks = ((p.animation_seconds + 1.) * 30.).ceil() as u64;
        if self
            .tick
            .checked_add(animation_ticks)
            .and_then(|v| v.checked_add(90))
            .is_none()
        {
            return Err(E::Invalid);
        }
        if p.corpse.id != p.corpse_item.id
            || p.corpse.id != p.corpse_container.id
            || p.corpse_item.revision != 0
            || p.corpse_item.place != bace_inventory::ItemPlace::World
            || !p.corpse_item.is_container
            || p.corpse_container.root_owner.is_some()
            || p.corpse_container.revision != 0
            || self.inventory.item(p.corpse.id).is_some()
            || self.inventory.container(p.corpse.id).is_some()
        {
            return Err(E::Invalid);
        }
        self.world
            .validate_player_corpse(p.actor, &p.corpse)
            .map_err(|_| E::MissingAssets)?;
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
        let (proposal, corpse_items, inventory_transcript) =
            self.death_inventory_plan(p, kind, level)?;
        let registry = self.magic.registry(p.actor).ok_or(E::MissingAssets)?;
        let value = registry
            .entries()
            .iter()
            .find(|e| e.spell == 666)
            .map(|e| e.spec.value);
        let vitae = if kind == PlayerDeathKind::Pkl {
            None
        } else {
            Some(
                self.death_policy
                    .next_vitae(level, value)
                    .map_err(|_| E::Invalid)?,
            )
        };
        let purge_bad =
            kind != PlayerDeathKind::Pk && self.death_int(p.actor, 232).unwrap_or(0) > 0;
        let mut updated = self
            .magic
            .prepare_death_registry(p.actor, vitae, kind, purge_bad)
            .map_err(|_| E::MissingAssets)?;
        let lost:Vec<_>=proposal.changes.iter().filter_map(|c| c.before.as_ref().filter(|b|matches!(b.place,bace_inventory::ItemPlace::Contained{equipped,..} if equipped!=0)).filter(|_|!matches!(c.after.place,bace_inventory::ItemPlace::Contained{container,equipped,..} if container==p.actor && equipped!=0)).map(|b|b.id)).collect();
        let removed: Vec<_> = updated
            .entries()
            .iter()
            .filter(|e| e.spell != 666 && lost.contains(&EntityId(e.caster)))
            .map(|e| (e.spell, e.spec.layer))
            .collect();
        if !removed.is_empty() {
            updated.remove(&removed).map_err(|_| E::Invalid)?;
        }
        let gear_health = p
            .equipped_health
            .iter()
            .filter(|(id, _)| !lost.contains(id))
            .try_fold(0u32, |sum, (_, v)| sum.checked_add(*v))
            .ok_or(E::Invalid)?;
        let post_death_maxima =
            self.death_vital_maxima(p.actor, &updated, p.vital_formulas, gear_health)?;
        let mut after = before.clone();
        if let Some(loot) = &p.olthoi {
            after.olthoi_loot_timestamp = loot.after_timestamp;
        }
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
        let location = self
            .accepted_portal_position(p.actor)
            .map_err(|_| E::Invalid)?;
        // Creature_Death.CreateCorpse updates only ordinary-avatar outdoor
        // deaths. Olthoi source loot is separately gated at selection.
        if p.olthoi.is_none() && location.cell & 0xffff < 0x100 {
            after.last_outside_death = Some(location);
        }
        let destination = death_destination(
            self.portal_links(p.actor).and_then(|l| l.position(4)),
            p.instantiation,
            location,
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
            .zip(post_death_maxima)
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
            .validate_player_respawn(p.actor, &vitals, post_death_maxima, None)
            .map_err(|_| E::Busy)?;
        let before_revision = self.characters.get(p.actor).ok_or(E::Invalid)?.revision();
        Ok(PlayerDeathTicket {
            operation: p.operation,
            actor: p.actor,
            killer: pending.killer,
            kind,
            olthoi: p.olthoi.as_ref().map(|p| p.kind),
            before_revision,
            after_revision: before_revision.checked_add(1).ok_or(E::Capacity)?,
            before,
            after,
            purge_bad,
            inventory: crate::InventoryTicket {
                operation: 0,
                actor: p.actor,
                proposal,
            },
            inventory_transcript,
            corpse: p.corpse.id,
            no_corpse: None,
            corpse_decay_seconds: bace_interactions::player_corpse_decay_seconds(
                level,
                corpse_items.is_empty(),
            ),
            corpse_items,
            registry_before_revision: registry.revision(),
            registry_after_revision: updated.revision(),
            before_enchantments: registry.entries().to_vec(),
            enchantments: updated.into_entries(),
            destination,
            vitals,
            post_death_maxima,
            animation_ticks,
        })
    }
}
