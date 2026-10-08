//! Authenticated crafting commands, sharing inventory ownership/reservations.
use super::*;
use bace_crafting::{
    CraftContext, CraftError, CraftItem, PreparedRecipe, SalvageRequest, TinkerChance,
};

impl Kernel {
    pub fn configure_crafting_random(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
    ) -> Result<(), CraftError> {
        self.crafting.configure(root)
    }
    pub fn quote_tinker(
        &mut self,
        action: ActionContext,
        context: &CraftContext,
        source: &CraftItem,
        target: &CraftItem,
        recipe: &PreparedRecipe,
        lifetime: u64,
    ) -> Result<TinkerChance, CraftError> {
        validate_actor_recipe(recipe)?;
        self.authorize_crafting(action, context.actor, context.actor_revision)?;
        self.crafting.quote(
            crate::crafting::QuoteInput {
                context,
                source,
                target,
                recipe,
                now: self.tick,
                lifetime,
            },
            &self.inventory,
        )
    }
    /// No-dialog source path: one authenticated use action, no fabricated second
    /// client sequence and no externally observable intermediate quote.
    pub fn execute_tinker(
        &mut self,
        action: ActionContext,
        context: &CraftContext,
        source: &CraftItem,
        target: &CraftItem,
        recipe: &PreparedRecipe,
    ) -> Result<u64, CraftError> {
        validate_actor_recipe(recipe)?;
        self.authorize_crafting(action, context.actor, context.actor_revision)?;
        self.crafting.quote(
            crate::crafting::QuoteInput {
                context,
                source,
                target,
                recipe,
                now: self.tick,
                lifetime: 1,
            },
            &self.inventory,
        )?;
        self.confirm_authorized_tinker(action, context, source, target, recipe)
    }
    pub fn confirm_tinker(
        &mut self,
        action: ActionContext,
        context: &CraftContext,
        source: &CraftItem,
        target: &CraftItem,
        recipe: &PreparedRecipe,
    ) -> Result<u64, CraftError> {
        validate_actor_recipe(recipe)?;
        self.authorize_crafting(action, context.actor, context.actor_revision)?;
        self.confirm_authorized_tinker(action, context, source, target, recipe)
    }
    pub(super) fn confirm_authorized_tinker(
        &mut self,
        action: ActionContext,
        context: &CraftContext,
        source: &CraftItem,
        target: &CraftItem,
        recipe: &PreparedRecipe,
    ) -> Result<u64, CraftError> {
        let registries = self.reserve_crafting_registries(action.actor)?;
        self.confirm_tinker_reserved(action, context, source, target, recipe, registries)
    }
    pub(super) fn confirm_tinker_reserved(
        &mut self,
        action: ActionContext,
        context: &CraftContext,
        source: &CraftItem,
        target: &CraftItem,
        recipe: &PreparedRecipe,
        registries: Vec<EntityId>,
    ) -> Result<u64, CraftError> {
        let result = if self
            .characters
            .get(action.actor)
            .is_none_or(|c| c.revision() != context.actor_revision)
        {
            Err(CraftError::StaleConfirmation)
        } else {
            self.crafting.confirm(
                context,
                source,
                target,
                recipe,
                self.tick,
                &mut self.inventory,
            )
        };
        match result {
            Ok(operation) => {
                self.crafting.attach_registries(operation, registries);
                let preflight = self
                    .stage_crafting_proficiency(operation, recipe)
                    .and_then(|()| self.preflight_crafting_retirement(operation))
                    .and_then(|()| {
                        self.prepare_crafting_wands(
                            self.crafting
                                .pending(operation)
                                .ok_or(CraftError::InvalidState)?,
                        )
                        .map(|_| ())
                    });
                if let Err(error) = preflight {
                    self.reject_crafting(operation)?;
                    return Err(error);
                }
                Ok(operation)
            }
            Err(error) => {
                self.release_crafting_registries(&registries)?;
                Err(error)
            }
        }
    }
    pub fn cancel_tinker(&mut self, action: ActionContext) -> Result<(), CraftError> {
        self.characters
            .authorize(action, self.world.body(action.actor).is_ok())
            .map_err(|_| CraftError::Ownership)?;
        self.crafting.cancel_quote(action.actor);
        Ok(())
    }
    pub fn propose_salvage(
        &mut self,
        action: ActionContext,
        request: SalvageRequest<'_>,
        generated: &[bace_inventory::InventoryItem],
    ) -> Result<crate::crafting::SalvageAdmission, CraftError> {
        self.authorize_crafting(action, request.actor, request.actor_revision)?;
        let registries = self.reserve_crafting_registries(action.actor)?;
        let result = if self
            .characters
            .get(action.actor)
            .is_none_or(|c| c.revision() != request.actor_revision)
        {
            Err(CraftError::StaleConfirmation)
        } else {
            self.stage_salvage(request, generated)
        };
        match result {
            Ok(crate::crafting::SalvageAdmission::Pending(operation)) => {
                self.crafting.attach_registries(operation, registries);
                if let Err(error) = self
                    .characters
                    .reserve_crafting(action.actor, operation)
                    .map_err(|_| CraftError::Busy)
                    .and_then(|()| self.preflight_crafting_retirement(operation))
                {
                    self.reject_crafting(operation)?;
                    return Err(error);
                }
                Ok(crate::crafting::SalvageAdmission::Pending(operation))
            }
            other => {
                self.release_crafting_registries(&registries)?;
                other
            }
        }
    }
    fn stage_salvage(
        &mut self,
        mut request: SalvageRequest<'_>,
        generated: &[bace_inventory::InventoryItem],
    ) -> Result<crate::crafting::SalvageAdmission, CraftError> {
        request.free_slots = self
            .inventory
            .free_slots(EntityId(request.actor), false)
            .map_err(|_| CraftError::InvalidState)? as usize;
        let decision = bace_crafting::propose_salvage(request)?;
        if decision.consumed.is_empty() {
            if !generated.is_empty() {
                return Err(CraftError::InvalidState);
            }
            return Ok(crate::crafting::SalvageAdmission::Empty(decision));
        }
        for item in generated {
            if (self.population.reserves_identity(item.id)
                || self.generator_reserves_identity(item.id))
                || self.magic.reserves_identity(item.id)
                || self.world.contains_identity(item.id)
            {
                return Err(CraftError::InvalidState);
            }
        }
        self.crafting
            .salvage(decision, generated, &mut self.inventory)
            .map(crate::crafting::SalvageAdmission::Pending)
    }
    pub fn take_crafting_proposal(&mut self) -> Option<crate::CraftingTicket> {
        self.crafting.take()
    }
    pub fn retry_crafting(&mut self, operation: u64) -> Result<(), CraftError> {
        self.crafting.retry(operation)
    }
    /// Only after definite rollback; uncertainty must retain the reservation.
    pub fn reject_crafting(&mut self, operation: u64) -> Result<(), CraftError> {
        let registries = self
            .crafting
            .pending(operation)
            .ok_or(CraftError::InvalidState)?
            .registry_reservations
            .clone();
        let actor = self
            .crafting
            .pending(operation)
            .ok_or(CraftError::InvalidState)?
            .actor;
        self.crafting.reject(operation, &mut self.inventory)?;
        self.release_crafting_proficiency(actor, operation)?;
        self.release_crafting_registries(&registries)
    }
    pub fn confirm_crafting_committed(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<crate::CraftingTicket, CraftError> {
        self.preflight_crafting_retirement(receipt.operation)?;
        self.validate_crafting_proficiency(receipt.operation)?;
        let pending = self
            .crafting
            .pending(receipt.operation)
            .ok_or(CraftError::InvalidState)?;
        let wands = self.prepare_crafting_wands(pending)?;
        let revision = match &pending.decision {
            crate::CraftingDecision::Tinker(p) => {
                Some((p.expected_actor_revision, p.actor_revision))
            }
            crate::CraftingDecision::Salvage(_) => None,
        };
        if self.characters.reserved(pending.actor)
            && self.characters.crafting_operation(pending.actor) != Some(receipt.operation)
        {
            return Err(CraftError::Busy);
        }
        if let Some((before, _)) = revision
            && self
                .characters
                .get(pending.actor)
                .is_none_or(|c| c.revision() != before)
        {
            return Err(CraftError::InvalidState);
        }
        let actor = pending.actor;
        let ticket = self.crafting.committed(receipt, &mut self.inventory)?;
        if ticket.proficiency.is_some() {
            self.adopt_crafting_proficiency(&ticket)?;
        } else if let Some((before, after)) = revision {
            self.release_crafting_proficiency(actor, receipt.operation)?;
            self.characters
                .adopt_crafting_revision(actor, before, after)
                .map_err(|_| CraftError::InvalidState)?;
        }
        self.release_crafting_proficiency(actor, receipt.operation)?;
        let now = self.tick as f64 / 30.0;
        for change in &ticket.inventory.changes {
            if change.after.place == bace_inventory::ItemPlace::Removed
                && self.magic.registry(change.after.id).is_some()
            {
                self.magic
                    .retire_item_registry(change.after.id, now)
                    .map_err(|_| CraftError::InvalidState)?;
                self.registry_revisions.remove(&change.after.id);
            }
        }
        let remaining: Vec<_> = ticket
            .registry_reservations
            .iter()
            .copied()
            .filter(|id| self.magic.registry(*id).is_some())
            .collect();
        self.release_crafting_registries(&remaining)?;
        for wand in wands {
            self.magic
                .refresh_damage_wand(wand)
                .map_err(|_| CraftError::InvalidState)?;
        }
        Ok(ticket)
    }
    pub fn has_crafting_state(&self) -> bool {
        self.crafting.has_state()
    }
    fn preflight_crafting_retirement(&self, operation: u64) -> Result<(), CraftError> {
        let ticket = self
            .crafting
            .pending(operation)
            .ok_or(CraftError::InvalidState)?;
        let now = self.tick as f64 / 30.0;
        for change in &ticket.inventory.changes {
            if change.after.place == bace_inventory::ItemPlace::Removed
                && self.magic.registry(change.after.id).is_some()
            {
                self.magic
                    .can_retire_item_registry(change.after.id, now)
                    .map_err(|_| CraftError::Busy)?;
            }
        }
        Ok(())
    }
    /// Freeze every owned inventory registry: removing an item can compact its
    /// siblings, whose placement and enchantments must be saved together too.
    pub(super) fn reserve_crafting_registries(
        &mut self,
        actor: EntityId,
    ) -> Result<Vec<EntityId>, CraftError> {
        let mut ids = vec![actor];
        ids.extend(
            self.inventory
                .items()
                .filter(|item| self.inventory.owned(actor, item.id))
                .map(|item| item.id),
        );
        ids.retain(|id| self.magic.registry(*id).is_some());
        ids.sort_unstable();
        ids.dedup();
        if ids
            .iter()
            .any(|id| self.inventory.reserved(*id) || self.magic.registry_reserved(*id))
        {
            return Err(CraftError::Busy);
        }
        let now = self.tick as f64 / 30.0;
        for (index, &id) in ids.iter().enumerate() {
            if self.magic.reserve_registry(id, true, now).is_err() {
                self.release_crafting_registries(&ids[..index])?;
                return Err(CraftError::Busy);
            }
        }
        if self.sync_registry_revisions().is_err() {
            self.release_crafting_registries(&ids)?;
            return Err(CraftError::InvalidState);
        }
        Ok(ids)
    }
    pub(super) fn release_crafting_registries(
        &mut self,
        ids: &[EntityId],
    ) -> Result<(), CraftError> {
        let now = self.tick as f64 / 30.0;
        for &id in ids {
            self.magic
                .reserve_registry(id, false, now)
                .map_err(|_| CraftError::InvalidState)?;
        }
        self.sync_registry_revisions()
            .map_err(|_| CraftError::InvalidState)
    }
    pub(super) fn authorize_crafting(
        &mut self,
        action: ActionContext,
        actor: u32,
        revision: u64,
    ) -> Result<(), CraftError> {
        if action.actor.0 != actor
            || self
                .characters
                .get(action.actor)
                .is_none_or(|c| c.revision() != revision)
        {
            return Err(CraftError::Ownership);
        }
        // The live owner decides peace/death state. A cold recipe context cannot
        // authorize crafting after the actor changes mode or dies.
        if self
            .world
            .combatant(action.actor)
            .is_none_or(|c| c.mode() != 1 || c.health() == 0)
        {
            return Err(CraftError::InvalidState);
        }
        if self.inventory.reserved(action.actor)
            || self.recall_busy(action.actor)
            || self.characters.reserved(action.actor)
            || self.npcs.reserved(action.actor)
            || self.magic.busy(action.actor)
        {
            return Err(CraftError::Busy);
        }
        self.characters
            .authorize(action, self.world.body(action.actor).is_ok())
            .map_err(|_| CraftError::Ownership)
    }
}

impl Kernel {
    /// Execute on the single simulation owner. The caller reserves output
    /// capacity before dequeueing and publishes this correlated outcome.
    pub fn apply_crafting_command(
        &mut self,
        command: crate::CraftingCommand,
    ) -> crate::CraftingOutcome {
        use crate::{CraftingCommandKind as C, CraftingResult as R};
        let correlation = command.correlation;
        let result = command
            .validate_bounds()
            .and_then(|()| match command.action {
                C::BeginUse {
                    context,
                    input,
                    motion,
                    quote,
                    lifetime,
                } => self.begin_tinker_motion(correlation, context, input, motion, quote, lifetime),
                C::Execute { context, input } => self
                    .execute_tinker(
                        context,
                        &input.context,
                        &input.source,
                        &input.target,
                        &input.recipe,
                    )
                    .map(R::Pending),
                C::Quote {
                    context,
                    input,
                    lifetime,
                } => self
                    .quote_tinker(
                        context,
                        &input.context,
                        &input.source,
                        &input.target,
                        &input.recipe,
                        lifetime,
                    )
                    .map(R::Quoted),
                C::Confirm { context, input } => self
                    .confirm_tinker(
                        context,
                        &input.context,
                        &input.source,
                        &input.target,
                        &input.recipe,
                    )
                    .map(R::Pending),
                C::Cancel { context } => self.cancel_tinker(context).map(|()| R::Cancelled),
                C::Salvage { context, input } => self
                    .propose_salvage(
                        context,
                        SalvageRequest {
                            actor: context.actor.0,
                            actor_revision: input.actor_revision,
                            operation_id: input.operation_id,
                            tool: input.tool,
                            skills: input.skills,
                            items: &input.items,
                            bag_templates: &input.bag_templates,
                            free_slots: 0,
                        },
                        &input.generated,
                    )
                    .map(|admission| match admission {
                        crate::SalvageAdmission::Empty(proposal) => {
                            R::SalvageEmpty(Box::new(proposal))
                        }
                        crate::SalvageAdmission::Pending(operation) => R::Pending(operation),
                    }),
                C::Commit { receipt } => self
                    .confirm_crafting_committed(&receipt)
                    .map(|ticket| R::Committed(Box::new(ticket))),
                C::Rollback { operation } => self
                    .reject_crafting(operation)
                    .map(|()| R::RolledBack(operation)),
                C::Retry { operation } => self
                    .retry_crafting(operation)
                    .map(|()| R::Retried(operation)),
            });
        crate::CraftingOutcome {
            correlation,
            result,
        }
    }
}

// Actor projections owned by progression, UI, magic or other subsystems require
// their explicit atomic adoption path. Merely editing their save would lose the
// update on the next live snapshot. Imbue statistics are retained sparse metadata.
pub(super) fn validate_actor_recipe(recipe: &PreparedRecipe) -> Result<(), CraftError> {
    for mutation in recipe
        .success
        .mutations
        .iter()
        .chain(&recipe.failure.mutations)
    {
        if mutation.participant == bace_crafting::Participant::Actor
            && !(mutation.key.kind == bace_crafting::PropertyKind::Int
                && matches!(mutation.key.id, 205 | 206))
        {
            return Err(CraftError::Unsupported);
        }
    }
    Ok(())
}
