//! One owner for the post-motion healer charge and self vital effects.
use super::*;
use crate::healing_kits::{
    HealingKitCommand as C, HealingKitDecision as D, HealingKitError as E, *,
};
use bace_entity::{EntityVital, VitalMutation};
use bace_gameplay_api::SkillAdvancement;
use bace_inventory::{InventoryView, ItemPlace};
use bace_motion::{MotionDomain, MotionExecutionEvent, MotionToken, PreparedMotionChain};
use bace_world::{VitalReservationDomain, VitalReservationToken};
use std::sync::Arc;

fn token(operation: u64) -> VitalReservationToken {
    VitalReservationToken {
        domain: VitalReservationDomain::HealingKit,
        operation,
    }
}

#[cfg(test)]
#[path = "healing_kits/tests.rs"]
mod tests;

impl Kernel {
    pub fn take_healing_kit_outcome(&mut self) -> Option<HealingKitOutcome> {
        self.healing_kits.outcomes.pop_front()
    }
    pub fn has_healing_kit_work(&self) -> bool {
        !self.healing_kits.pending.is_empty()
            || !self.healing_kits.animations.is_empty()
            || !self.healing_kits.returns.is_empty()
            || !self.healing_kits.outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::HealingKit(_)))
    }
    pub(super) fn apply_healing_kit_command(&mut self, command: C) -> HealingKitOutcome {
        let correlation = command.correlation();
        let context = match &command {
            C::Use { context, .. } => Some(*context),
            _ => None,
        };
        let result = match command {
            C::Use {
                context,
                item,
                item_revision,
                target,
                profile,
                motion,
                ..
            } => self.begin_healing_motion(
                correlation,
                context,
                item,
                item_revision,
                target,
                profile,
                motion,
            ),
            C::Commit {
                operation, receipt, ..
            } => self
                .commit_healing_kit(operation, receipt.as_ref())
                .map(D::Committed),
            C::Reject { operation, .. } => self
                .reject_healing_kit(operation)
                .map(|()| D::Rejected(operation)),
        };
        HealingKitOutcome {
            correlation,
            context,
            result,
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "one source-prepared authenticated healing Use"
    )]
    fn begin_healing_motion(
        &mut self,
        correlation: u64,
        context: ActionContext,
        item: EntityId,
        item_revision: u64,
        target: EntityId,
        profile: PreparedHealingKit,
        motion: Arc<PreparedMotionChain>,
    ) -> Result<D, E> {
        if target != context.actor {
            return Err(E::UnsupportedTarget);
        }
        if self.healing_kits.animations.len() >= self.healing_kits.capacity.min(64)
            || self.healing_kits.animations.contains_key(&context.actor)
            || self.characters.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.world.has_reserved_vitals(context.actor)
            || self.magic.busy(context.actor)
        {
            return Err(E::Busy);
        }
        let source = motion.source_transition().ok_or(E::Content)?;
        if motion.motion != 0x1000_010e || motion.speed != 1.0 || source.continues_cycle {
            return Err(E::Content);
        }
        let kit = self.inventory.item(item).ok_or(E::Ownership)?;
        if !self.inventory.owned(context.actor, item)
            || !matches!(kit.place, ItemPlace::Contained { equipped: 0, .. })
        {
            return Err(E::Ownership);
        }
        if kit.revision != item_revision || self.inventory.reserved(item) {
            return Err(E::Stale);
        }
        if !profile.unlimited_use
            && (profile.max_structure.is_none_or(|max| {
                max == 0 || kit.structure.is_none_or(|uses| uses == 0 || uses > max)
            }) || profile.template_value.is_none())
        {
            return Err(E::Content);
        }
        let skill = self
            .combat
            .skills
            .get(&context.actor)
            .and_then(|profile| profile.values.get(&21))
            .ok_or(E::NotTrained)?;
        if (skill.advancement as u32) < (SkillAdvancement::Trained as u32) {
            return Err(E::NotTrained);
        }
        let vital = self
            .world
            .vital(target, profile.vital)
            .map_err(|_| E::Content)?;
        if vital.current >= vital.maximum {
            return Err(E::FullVital);
        }
        let (cell, accepted) = self
            .world
            .actor_state(context.actor)
            .map_err(|_| E::Ownership)?;
        if !accepted.grounded() {
            return Err(E::Busy);
        }
        self.characters
            .authorize(context, true)
            .map_err(|_| E::Ownership)?;
        let token = MotionToken {
            domain: MotionDomain::Interaction,
            owner: correlation,
            sequence: 1,
        };
        self.world
            .begin_motion(context.actor, token, motion)
            .map_err(|_| E::Busy)?;
        self.healing_kits.animations.insert(
            context.actor,
            HealingAnimation {
                correlation,
                context,
                item,
                item_revision,
                target,
                profile,
                token,
                start_cell: cell,
                start_position: accepted.position(),
            },
        );
        Ok(D::MotionStarted {
            actor: context.actor,
            style: source.before.style,
            command: 0x1000_010e,
        })
    }
    pub(super) fn step_healing_kits(&mut self) -> Result<(), SimulationError> {
        let returning: Vec<_> = self
            .healing_kits
            .returns
            .iter()
            .map(|(&actor, &token)| (actor, token))
            .collect();
        for (actor, token) in returning {
            for _ in 0..128 {
                let Some(event) = self.world.take_motion_event_matching(actor, token) else {
                    break;
                };
                if matches!(
                    event.event,
                    MotionExecutionEvent::Completed | MotionExecutionEvent::Cancelled
                ) {
                    self.healing_kits.completed_returns.insert(actor);
                }
            }
            if self.world.source_motion_token(actor) != Some(token)
                || self.healing_kits.completed_returns.contains(&actor)
                    && self.world.can_retire_actor_motion(actor)
                    && self.world.retire_actor_motion(actor).is_ok()
            {
                self.healing_kits.returns.remove(&actor);
                self.healing_kits.completed_returns.remove(&actor);
            }
        }
        let actors: Vec<_> = self.healing_kits.animations.keys().copied().collect();
        for actor in actors {
            if self.healing_kits.outcomes.len() >= self.healing_kits.capacity {
                break;
            }
            let token = self
                .healing_kits
                .animations
                .get(&actor)
                .expect("healing animation owner")
                .token;
            let mut finished = None;
            for _ in 0..128 {
                let Some(event) = self.world.take_motion_event_matching(actor, token) else {
                    break;
                };
                match event.event {
                    MotionExecutionEvent::Completed => {
                        finished = Some(true);
                        break;
                    }
                    MotionExecutionEvent::Cancelled => {
                        finished = Some(false);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(completed) = finished else {
                continue;
            };
            self.world
                .end_interaction_motion(actor, token.owner)
                .map_err(|_| SimulationError::InvalidCommand)?;
            if completed {
                self.healing_kits.returns.insert(
                    actor,
                    MotionToken {
                        sequence: token
                            .sequence
                            .checked_add(1)
                            .ok_or(SimulationError::InvalidCommand)?,
                        ..token
                    },
                );
            }
            let animation = self
                .healing_kits
                .animations
                .remove(&actor)
                .expect("healing animation owner");
            let unmoved = self.world.actor_state(actor).is_ok_and(|(cell, state)| {
                cell == animation.start_cell
                    && (state.position() - animation.start_position).length_squared() < 25.0
            });
            let npk = self
                .player_death_state(actor)
                .is_some_and(|state| state.pk_status == 2);
            let result = if completed && (unmoved || npk) {
                self.propose_healing_kit(
                    animation.context,
                    animation.item,
                    animation.item_revision,
                    animation.target,
                    animation.profile,
                    true,
                )
                .map(D::Proposed)
            } else {
                Err(E::MotionDisrupted)
            };
            self.healing_kits.outcomes.push_back(HealingKitOutcome {
                correlation: animation.correlation,
                context: Some(animation.context),
                result,
            });
        }
        Ok(())
    }
    fn propose_healing_kit(
        &mut self,
        context: ActionContext,
        item: EntityId,
        item_revision: u64,
        target: EntityId,
        profile: PreparedHealingKit,
        authorized: bool,
    ) -> Result<HealingKitTicket, E> {
        if target != context.actor {
            return Err(E::UnsupportedTarget);
        }
        if !profile.healkit_mod.is_finite() || !(0.0..=1000.0).contains(&profile.healkit_mod) {
            return Err(E::Content);
        }
        if !authorized {
            self.characters
                .authorize(context, self.world.body(context.actor).is_ok())
                .map_err(|_| E::Ownership)?;
        } else if !self
            .characters
            .binding_matches(bace_gameplay_api::CharacterBinding {
                actor: context.actor,
                account: context.account,
                session: context.session,
            })
        {
            return Err(E::Ownership);
        }
        if self.characters.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.magic.busy(context.actor)
            || self.world.has_reserved_vitals(context.actor)
            || self.combat.active(context.actor)
            || self.healing_kits.pending.len() >= self.healing_kits.capacity
        {
            return Err(E::Busy);
        }
        self.prepare_inventory_time().map_err(|_| E::Busy)?;
        let kit = self.inventory.item(item).ok_or(E::Ownership)?;
        if !self.inventory.owned(context.actor, item)
            || !matches!(kit.place, ItemPlace::Contained { equipped: 0, .. })
        {
            return Err(E::Ownership);
        }
        if kit.revision != item_revision || self.inventory.reserved(item) {
            return Err(E::Stale);
        }
        if let Some(requirements) = &profile.activation {
            self.check_inventory_item_activation(context.actor, requirements)
                .map_err(|_| E::Ownership)?;
        }
        let skill = self
            .combat
            .skills
            .get(&context.actor)
            .and_then(|profile| profile.values.get(&21))
            .copied()
            .ok_or(E::NotTrained)?;
        if (skill.advancement as u32) < (SkillAdvancement::Trained as u32) {
            return Err(E::NotTrained);
        }
        let pool = self
            .world
            .vital(target, profile.vital)
            .map_err(|_| E::Content)?;
        if pool.current >= pool.maximum {
            return Err(E::FullVital);
        }
        let stamina = self
            .world
            .vital(context.actor, EntityVital::Stamina)
            .map_err(|_| E::Content)?;
        let check = self
            .project_healing_check(context.actor, profile.boost_value, profile.vital)
            .map_err(|_| E::Content)?;
        let rating = self
            .magic
            .healing_rating_modifier(target)
            .map_err(|_| E::Content)?;
        let proposal = if profile.unlimited_use {
            // Reserve the source item without manufacturing a changed item row.
            // The player-only durable write still fences a live source kit.
            bace_inventory::InventoryProposal {
                changes: Vec::new(),
                participants: vec![(item, item_revision)],
                actor_burden: 0,
                requires_pickup_motion: false,
            }
        } else {
            let items: Vec<_> = self
                .inventory
                .items()
                .filter(|candidate| self.inventory.owned(context.actor, candidate.id))
                .cloned()
                .collect();
            let containers: Vec<_> = self
                .inventory
                .containers()
                .filter(|container| container.root_owner == Some(context.actor))
                .copied()
                .collect();
            bace_inventory::propose_healing_kit_charge(
                context.actor,
                item,
                profile.max_structure,
                profile.template_value,
                InventoryView {
                    items: &items,
                    containers: &containers,
                },
            )
            .map_err(|_| E::Content)?
        };
        let operation = self
            .inventory
            .reserve(context.actor, proposal)
            .map_err(|_| E::Busy)?;
        if self.reserve_inventory_registries(operation, &[]).is_err() {
            self.inventory.reject(operation).map_err(|_| E::Receipt)?;
            return Err(E::Busy);
        }
        self.inventory.claim(operation).map_err(|_| E::Receipt)?;
        let inventory = if profile.unlimited_use {
            None
        } else {
            Some(
                self.inventory
                    .pending_ticket(operation)
                    .ok_or(E::Receipt)?
                    .clone(),
            )
        };
        let before = match self
            .characters
            .reserve_healing_kit(context.actor, operation)
        {
            Ok(before) => before,
            Err(()) => {
                self.release_inventory_registries(operation)
                    .map_err(|_| E::Receipt)?;
                self.inventory.reject(operation).map_err(|_| E::Receipt)?;
                return Err(E::Busy);
            }
        };
        let computed = self.healing_vitals(
            operation,
            context.actor,
            profile.vital,
            profile.healkit_mod,
            rating,
            skill.current,
            pool,
            stamina,
            check,
        );
        let (success, critical, amount, cost, vitals) = match computed {
            Ok(value) => value,
            Err(error) => {
                self.characters
                    .reject_healing_kit(context.actor, operation, before)
                    .map_err(|_| E::Receipt)?;
                self.release_inventory_registries(operation)
                    .map_err(|_| E::Receipt)?;
                self.inventory.reject(operation).map_err(|_| E::Receipt)?;
                return Err(error);
            }
        };
        if !vitals.is_empty() {
            let resources: Vec<_> = vitals
                .iter()
                .map(|change| (change.actor, change.vital))
                .collect();
            if self
                .world
                .reserve_vitals(&resources, token(operation))
                .is_err()
                || self
                    .world
                    .validate_vital_batch_reserved(&vitals, None, token(operation))
                    .is_err()
            {
                self.world.release_vitals(token(operation));
                self.characters
                    .reject_healing_kit(context.actor, operation, before)
                    .map_err(|_| E::Receipt)?;
                self.release_inventory_registries(operation)
                    .map_err(|_| E::Receipt)?;
                self.inventory.reject(operation).map_err(|_| E::Receipt)?;
                return Err(E::Busy);
            }
        }
        let ticket = HealingKitTicket {
            operation,
            context,
            target,
            inventory,
            actor_revision: before,
            vitals,
            success,
            critical,
            heal_amount: amount,
            stamina_cost: cost,
        };
        self.healing_kits.pending.insert(operation, ticket.clone());
        Ok(ticket)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "ACE DoHealing receives independently owned live skill, item, rating and vital projections"
    )]
    fn healing_vitals(
        &self,
        operation: u64,
        actor: EntityId,
        vital: EntityVital,
        mod_value: f32,
        rating: f32,
        skill: u32,
        pool: bace_entity::VitalPool,
        stamina: bace_entity::VitalPool,
        check: bace_combat::specialization::HealingCheck,
    ) -> Result<(bool, bool, u32, u32, Vec<VitalMutation>), E> {
        let mut stream = self
            .combat
            .healing_stream(operation)
            .map_err(|_| E::Content)?;
        let draw = |stream: &mut bace_random::RandomStream| -> Result<f32, E> {
            Ok((stream.next_u64().map_err(|_| E::Capacity)? >> 40) as f32 / (1u32 << 24) as f32)
        };
        let chance = bace_combat::skill_chance(check.effective_skill, check.difficulty)
            .map_err(|_| E::Content)?;
        let success = f64::from(draw(&mut stream)?) < chance;
        if !success {
            return Ok((false, false, 0, 0, Vec::new()));
        }
        let base = skill as f32 * mod_value;
        let mut raw = base * (0.2 + draw(&mut stream)? * 0.3);
        let critical = draw(&mut stream)? < 0.1;
        if critical {
            raw *= 2.0;
        }
        raw = raw.min((pool.maximum - pool.current) as f32);
        if !raw.is_finite() || raw < 0.0 {
            return Err(E::Content);
        }
        let mut cost = (raw / 5.0).round_ties_even() as u32;
        if cost > stamina.current {
            cost = stamina.current;
            raw = (cost * 5) as f32;
        }
        let amount = (raw * rating).round_ties_even() as u32;
        let mut vitals = Vec::new();
        if cost > 0 {
            vitals.push(VitalMutation {
                actor,
                vital: EntityVital::Stamina,
                before: stamina.current,
                after: stamina.current - cost,
            });
        }
        if amount > 0 {
            if vital == EntityVital::Stamina
                && let Some(existing) = vitals.iter_mut().find(|v| v.vital == EntityVital::Stamina)
            {
                existing.after = existing.after.saturating_add(amount).min(stamina.maximum);
            } else {
                vitals.push(VitalMutation {
                    actor,
                    vital,
                    before: pool.current,
                    after: pool.current.saturating_add(amount).min(pool.maximum),
                });
            }
        }
        Ok((true, critical, amount, cost, vitals))
    }
    fn commit_healing_kit(
        &mut self,
        operation: u64,
        receipt: Option<&crate::InventoryReceipt>,
    ) -> Result<HealingKitTicket, E> {
        let ticket = self
            .healing_kits
            .pending
            .get(&operation)
            .ok_or(E::Receipt)?
            .clone();
        self.characters
            .validate_healing_kit(ticket.context.actor, operation, ticket.actor_revision)
            .map_err(|_| E::Receipt)?;
        match (&ticket.inventory, receipt) {
            (Some(held), Some(receipt)) if held.operation == operation => {
                self.inventory
                    .validate_receipt(receipt)
                    .map_err(|_| E::Receipt)?;
            }
            (None, None) => {
                self.inventory
                    .validate_receipt(&crate::InventoryReceipt {
                        operation,
                        revisions: Vec::new(),
                    })
                    .map_err(|_| E::Receipt)?;
            }
            _ => return Err(E::Receipt),
        }
        self.preflight_inventory_registries(operation, true)
            .map_err(|_| E::Receipt)?;
        if !ticket.vitals.is_empty() {
            self.world
                .validate_vital_batch_reserved(&ticket.vitals, None, token(operation))
                .map_err(|_| E::Receipt)?;
        }
        self.characters
            .commit_healing_kit(ticket.context.actor, operation, ticket.actor_revision)
            .map_err(|_| E::Receipt)?;
        if !ticket.vitals.is_empty() {
            self.world
                .apply_vital_batch_reserved(&ticket.vitals, None, token(operation))
                .expect("single-owner healing vital preflight");
        }
        if let (Some(held), Some(receipt)) = (&ticket.inventory, receipt) {
            self.inventory
                .confirm(receipt)
                .expect("single-owner healing item preflight");
            self.retire_inventory_registries(held)
                .map_err(|_| E::Receipt)?;
        } else {
            self.inventory
                .confirm(&crate::InventoryReceipt {
                    operation,
                    revisions: Vec::new(),
                })
                .expect("single-owner unlimited kit hold preflight");
        }
        self.release_inventory_registries(operation)
            .map_err(|_| E::Receipt)?;
        self.world.release_vitals(token(operation));
        self.healing_kits.pending.remove(&operation);
        Ok(ticket)
    }
    fn reject_healing_kit(&mut self, operation: u64) -> Result<(), E> {
        let ticket = self
            .healing_kits
            .pending
            .get(&operation)
            .ok_or(E::Receipt)?
            .clone();
        self.characters
            .validate_healing_kit(ticket.context.actor, operation, ticket.actor_revision)
            .map_err(|_| E::Receipt)?;
        let held = self.inventory.pending_ticket(operation).ok_or(E::Receipt)?;
        if ticket
            .inventory
            .as_ref()
            .is_some_and(|source| source != held)
            || ticket.inventory.is_none() && !held.proposal.changes.is_empty()
        {
            return Err(E::Receipt);
        }
        self.release_inventory_registries(operation)
            .map_err(|_| E::Receipt)?;
        self.inventory.reject(operation).map_err(|_| E::Receipt)?;
        self.characters
            .reject_healing_kit(ticket.context.actor, operation, ticket.actor_revision)
            .map_err(|_| E::Receipt)?;
        self.world.release_vitals(token(operation));
        self.healing_kits.pending.remove(&operation);
        Ok(())
    }
}
