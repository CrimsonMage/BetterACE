//! One Population death owner: durable corpse/reward preparation and trusted
//! animation-gated completion. Source entities stay owned until the exact receipt.
use super::*;
use std::collections::BTreeSet;
impl Population {
    pub(crate) fn submitted_death(&self, operation: u64) -> Option<&DeathProposal> {
        self.pending
            .get(&operation)
            .filter(|pending| pending.submitted)
            .map(|pending| &pending.proposal)
    }
    pub(crate) fn staged_death(&self, operation: u64) -> Option<&PreparedPveDeath> {
        self.pending.get(&operation)?.prepared.as_ref()
    }
    pub(crate) fn stage_death(
        &mut self,
        operation: u64,
        prepared: PreparedPveDeath,
    ) -> Result<(), PveError> {
        let pending = self
            .pending
            .get_mut(&operation)
            .filter(|pending| pending.submitted && pending.prepared.is_none())
            .ok_or(PveError::InvalidReceipt)?;
        pending.prepared = Some(prepared);
        Ok(())
    }
    /// Called before draining a death event. Failure retains that event and all
    /// its world state; no random input or durable operation ID is consumed.
    pub(crate) fn ingest(
        &mut self,
        event: CombatEvent,
        world: &World,
        characters: &mut Characters,
        tick: u64,
        reward_policy: (bool, bool),
        is_olthoi: impl Fn(EntityId) -> bool,
    ) -> bool {
        let (shared_rewards, suppress_experience) = reward_policy;
        let CombatEvent::Damage {
            target,
            killed: true,
            ..
        } = event
        else {
            return true;
        };
        let Some(npc) = self.npcs.get(&target) else {
            return true;
        };
        if self.pending.len() >= self.capacity {
            return false;
        }
        let Some(state) = world.combatant(target) else {
            return false;
        };
        let Ok((cell, accepted)) = world.actor_state(target) else {
            return false;
        };
        let point = accepted.position();
        let half = accepted.heading_radians() * 0.5;
        let position = bace_content::Position {
            obj_cell_id: cell.0,
            position_x: point.x,
            position_y: point.y,
            position_z: point.z,
            rotation_w: half.cos(),
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: half.sin(),
        };
        let no_corpse = matches!(
            world
                .properties(target)
                .and_then(|p| p.get(bace_entity::PropertyFamily::Bool, 29)),
            Some(bace_entity::PropertyValue::Bool(true))
        );
        let shares: Vec<_> = state
            .contributors()
            .iter()
            .map(|(actor, damage)| DamageShare {
                actor: actor.0,
                damage: *damage,
                eligible: characters.get(*actor).is_some()
                    && world.combatant(*actor).is_some_and(|s| s.profile().player),
            })
            .collect();
        let mut rewards = Vec::with_capacity(shares.len());
        if kill_rewards(&shares, npc.blueprint.xp_override, &mut rewards).is_err() {
            return false;
        }
        let mut owner = None;
        let mut largest = -1.0_f32;
        for (actor, damage) in state.contributors() {
            if *damage > largest {
                largest = *damage;
                owner = Some(*actor);
            }
        }
        let olthoi_killer = owner.is_some_and(is_olthoi);
        let native = match if no_corpse && olthoi_killer {
            self.native.prepare_empty_no_corpse(target)
        } else {
            self.native.prepare(target, owner, world, characters, tick)
        } {
            Ok(value) => {
                self.native_error = None;
                value
            }
            Err(error) => {
                self.native_error = Some(error);
                return false;
            }
        };
        let (drops, consumed) = if let Some(generated) = &native {
            (
                generated
                    .generated
                    .iter()
                    .map(|drop| LootDrop {
                        template: drop.template,
                        stack: drop.stack,
                    })
                    .collect(),
                0,
            )
        } else {
            // Explicit legacy synthetic fixture path only; native policies never
            // read or advance this shared injected draw queue.
            let eligible: Vec<_> = npc
                .blueprint
                .loot
                .iter()
                .filter(|r| {
                    r.destination & 1 != 0 || (r.destination & 8 != 0 && r.destination & 2 == 0)
                })
                .collect();
            let entries: Vec<_> = eligible
                .iter()
                .map(|r| CreateEntry {
                    template: r.template,
                    destination: r.destination,
                    shade: r.probability,
                })
                .collect();
            let mut selected = Vec::with_capacity(entries.len());
            let Ok(consumed) =
                select_create_list(&entries, self.random.make_contiguous(), &mut selected)
            else {
                return false;
            };
            (
                selected
                    .into_iter()
                    .filter_map(|index| {
                        let row = eligible[index];
                        (row.template != 0).then_some(LootDrop {
                            template: row.template,
                            stack: row.stack,
                        })
                    })
                    .collect(),
                consumed,
            )
        };
        let Some(operation) = self.operation.checked_add(1) else {
            return false;
        };
        let Some(ready) = tick.checked_add(npc.blueprint.death_animation_ticks) else {
            return false;
        };
        let experience: Vec<_> = rewards
            .into_iter()
            .map(|(id, xp)| (EntityId(id), if suppress_experience { 0 } else { xp }))
            .collect();
        let experience_state = if shared_rewards {
            Vec::new()
        } else {
            let Some(experience_state) = characters
                .prepare_native_rewards(&experience, native.as_ref().and_then(|n| n.rare.as_ref()))
            else {
                return false;
            };
            characters.reserve_native_rewards(
                operation,
                &experience_state,
                native.as_ref().and_then(|n| n.rare.as_ref()),
            );
            experience_state
        };
        let proposal = DeathProposal {
            operation,
            native,
            victim: target,
            owner,
            position: Some(position),
            no_corpse,
            olthoi_killer,
            corpse_template: npc.blueprint.corpse_template,
            corpse_decay_ticks: npc.blueprint.corpse_decay_ticks,
            drops,
            experience,
            experience_state,
            social: None,
        };
        self.random.drain(..consumed);
        self.operation = operation;
        self.pending.insert(
            operation,
            PendingDeath {
                death_motion: None,
                shared_waiting: shared_rewards,
                origin: npc.origin,
                proposal,
                blueprint: npc.blueprint.clone(),
                ready,
                submitted: false,
                prepared: None,
            },
        );
        true
    }
    pub(crate) fn retry(&mut self, operation: u64) -> Result<(), PveError> {
        let pending = self
            .pending
            .get_mut(&operation)
            .ok_or(PveError::UnknownOperation)?;
        pending.submitted = false;
        Ok(())
    }
    /// Only the trusted persistence adapter calls this after confirmed commit of
    /// corpse/items/rewards with the same durable operation ID. Failures retain
    /// pending state and can be retried; XP is never optimistically granted here.
    pub(crate) fn committed(
        &mut self,
        operation: u64,
        corpse: EntityId,
        items: &[EntityId],
        credits: &[(EntityId, ExperienceCredit)],
        world: &mut World,
        tick: u64,
    ) -> Result<(), PveError> {
        let pending = self
            .pending
            .get(&operation)
            .ok_or(PveError::UnknownOperation)?;
        if pending.proposal.no_corpse {
            return Err(PveError::InvalidReceipt);
        }
        if credits != pending.proposal.experience_state
            || !pending.submitted
            || match pending.death_motion {
                Some((token, epoch)) => {
                    !world.death_motion_complete(pending.proposal.victim, token, epoch)
                }
                None => tick < pending.ready,
            }
            || items.len() != pending.proposal.drops.len()
            || corpse.0 == 0
            || self.reserves_identity(corpse)
        {
            return Err(PveError::InvalidReceipt);
        }
        for (index, id) in items.iter().enumerate() {
            if id.0 == 0
                || *id == corpse
                || world.contains_identity(*id)
                || items[..index].contains(id)
                || self.reserves_identity(*id)
            {
                return Err(PveError::InvalidReceipt);
            }
        }
        if self.events.len() == self.capacity
            || (pending.origin.is_none() && self.respawns.pending() == self.capacity)
            || (pending.origin.is_some() && self.generated_deaths.len() == self.capacity)
            || self.decays.pending() == self.capacity
        {
            return Err(PveError::Capacity);
        }
        let source = pending.proposal.victim;
        if pending.origin.is_none() {
            self.respawns
                .schedule(
                    source.0,
                    0,
                    operation,
                    tick,
                    pending.blueprint.respawn_ticks,
                )
                .map_err(|_| PveError::Overflow)?;
        }
        if self
            .decays
            .schedule(
                corpse.0,
                0,
                operation,
                tick,
                pending.blueprint.corpse_decay_ticks,
            )
            .is_err()
        {
            self.respawns.cancel_generator(source.0, operation);
            return Err(PveError::Overflow);
        }
        let state = CorpseState {
            operation,
            source,
            template: pending.proposal.corpse_template,
            owner: pending.proposal.owner,
            items: items.to_vec(),
        };
        if world.create_corpse(source, corpse, state).is_err() {
            self.respawns.cancel_generator(source.0, operation);
            self.decays.cancel_generator(corpse.0, operation);
            return Err(PveError::InvalidReceipt);
        }
        if let Some(origin) = pending.origin {
            self.generated_deaths.push_back(GeneratedNpcDeath {
                actor: source,
                origin,
            });
            self.native.retired(source);
        } else {
            self.respawn_blueprints
                .insert(source.0, pending.blueprint.clone());
            self.native.committed(source);
        }
        self.npcs.remove(&source);
        self.pending.remove(&operation);
        self.proposals.retain(|p| p.operation != operation);
        self.events
            .push_back(PveEvent::CorpseCreated { operation, corpse });
        Ok(())
    }
    /// Exact prepared world forest was staged before SQL. The post-commit path
    /// rechecks current owners and returns its forest for same-owner inventory,
    /// registry and expiry adoption. A rejected world handoff retains the stage.
    pub(crate) fn committed_prepared(
        &mut self,
        operation: u64,
        corpse: Option<EntityId>,
        items: &[EntityId],
        credits: &[(EntityId, ExperienceCredit)],
        world: &mut World,
        tick: u64,
    ) -> Result<crate::PreparedWorldRegionItems, PveError> {
        let pending = self
            .pending
            .get(&operation)
            .ok_or(PveError::UnknownOperation)?;
        let staged = pending.prepared.as_ref().ok_or(PveError::InvalidReceipt)?;
        let expected_corpse = (!pending.proposal.no_corpse)
            .then(|| staged.forest.roots.first().map(|root| root.entity))
            .flatten();
        let expected_items: BTreeSet<_> = staged
            .forest
            .items
            .iter()
            .map(|item| item.id)
            .filter(|id| Some(*id) != expected_corpse)
            .collect();
        if corpse != expected_corpse
            || (pending.proposal.no_corpse && corpse.is_some())
            || (!pending.proposal.no_corpse && corpse.is_none())
            || items.len() != pending.proposal.drops.len()
            || expected_items != items.iter().copied().collect()
            || credits != pending.proposal.experience_state
            || !pending.submitted
            || match pending.death_motion {
                Some((token, epoch)) => {
                    !world.death_motion_complete(pending.proposal.victim, token, epoch)
                }
                None => tick < pending.ready,
            }
            || self.events.len() == self.capacity
            || (pending.origin.is_none() && self.respawns.pending() == self.capacity)
            || (pending.origin.is_some() && self.generated_deaths.len() == self.capacity)
            || corpse.is_some() && self.decays.pending() == self.capacity
        {
            return Err(PveError::InvalidReceipt);
        }
        world
            .preflight_pve_death_roots(pending.proposal.victim, &staged.roots)
            .map_err(|_| PveError::InvalidReceipt)?;
        let source = pending.proposal.victim;
        let origin = pending.origin;
        let blueprint = pending.blueprint.clone();
        if pending.origin.is_none() {
            self.respawns
                .schedule(
                    source.0,
                    0,
                    operation,
                    tick,
                    pending.blueprint.respawn_ticks,
                )
                .map_err(|_| PveError::Overflow)?;
        }
        if let Some(corpse) = corpse
            && self
                .decays
                .schedule(
                    corpse.0,
                    0,
                    operation,
                    tick,
                    pending.blueprint.corpse_decay_ticks,
                )
                .is_err()
        {
            self.respawns.cancel_generator(source.0, operation);
            return Err(PveError::Overflow);
        }
        let mut prepared = self
            .pending
            .get_mut(&operation)
            .expect("preflighted pending death")
            .prepared
            .take()
            .expect("preflighted prepared death");
        if let Err((_, roots)) = world.replace_dead_pve_with_roots(source, prepared.roots) {
            prepared.roots = roots;
            self.pending
                .get_mut(&operation)
                .expect("retained pending death")
                .prepared = Some(prepared);
            self.respawns.cancel_generator(source.0, operation);
            if let Some(corpse) = corpse {
                self.decays.cancel_generator(corpse.0, operation);
            }
            return Err(PveError::InvalidReceipt);
        }
        if let Some(origin) = origin {
            self.generated_deaths.push_back(GeneratedNpcDeath {
                actor: source,
                origin,
            });
            self.native.retired(source);
        } else {
            self.respawn_blueprints.insert(source.0, blueprint);
            self.native.committed(source);
        }
        let root_ids = prepared
            .forest
            .roots
            .iter()
            .map(|root| root.entity)
            .collect();
        self.npcs.remove(&source);
        self.pending.remove(&operation);
        self.proposals
            .retain(|proposal| proposal.operation != operation);
        self.events.push_back(if let Some(corpse) = corpse {
            PveEvent::CorpseCreated { operation, corpse }
        } else {
            PveEvent::NoCorpseWorldDropsCreated {
                operation,
                victim: source,
                roots: root_ids,
            }
        });
        Ok(prepared.forest)
    }
    pub(crate) fn maintain(&mut self, world: &mut World, tick: u64) {
        for pending in self.pending.values_mut() {
            if self.proposals.len() == self.capacity {
                break;
            }
            if world.has_death_motions(pending.proposal.victim) {
                if pending.death_motion.is_none() {
                    let Ok(token) = world.begin_death_motion(pending.proposal.victim) else {
                        continue;
                    };
                    let Ok(body) = world.body(pending.proposal.victim) else {
                        continue;
                    };
                    pending.death_motion = Some((token, body.accepted().epoch()));
                }
                if pending.death_motion.is_some_and(|(token, epoch)| {
                    !world.death_motion_complete(pending.proposal.victim, token, epoch)
                }) {
                    continue;
                }
            }
            if !pending.shared_waiting
                && !pending.submitted
                && (pending.death_motion.is_some() || tick >= pending.ready)
            {
                self.proposals.push_back(pending.proposal.clone());
                pending.submitted = true;
            }
        }
        if self.events.len() < self.capacity
            && let Some(ticket) = self.respawns.next_due(tick)
            && let Some(&id) = self.ids.front()
            && let Some(blueprint) = self.respawn_blueprints.get(&ticket.generator).cloned()
            && self.native.respawn_ready(ticket.generator, id)
            && self.admit(id, blueprint, world, tick, None).is_ok()
        {
            self.ids.pop_front();
            self.native.respawned(ticket.generator, id);
            self.respawns.acknowledge(ticket);
            self.respawn_blueprints.remove(&ticket.generator);
            self.events.push_back(PveEvent::Respawned {
                previous: EntityId(ticket.generator),
                actor: id,
            });
        }
        if self.events.len() < self.capacity
            && let Some(ticket) = self.decays.next_due_matching(tick, |ticket| {
                world
                    .corpse(EntityId(ticket.generator))
                    .is_some_and(|state| state.items.is_empty())
            })
        {
            let corpse = EntityId(ticket.generator);
            if world
                .corpse(corpse)
                .is_some_and(|state| state.items.is_empty())
            {
                world.remove(corpse);
                self.decays.acknowledge(ticket);
                self.events.push_back(PveEvent::CorpseDecayed { corpse });
            }
        }
    }
}
