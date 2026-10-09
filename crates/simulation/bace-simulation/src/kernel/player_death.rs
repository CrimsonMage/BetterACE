//! Durable player death orchestration; accepted state changes follow exact receipts.
mod completion;
mod consent;
mod corpse_access;
mod no_corpse;
mod olthoi;
mod preparation;
#[cfg(test)]
#[path = "player_death/protection_tests.rs"]
mod protection_tests;
mod selection;
mod timers;
mod vitals;
use super::*;
use crate::player_death::{PlayerDeathError as E, PlayerDeathState, PlayerVitaeRecovery};
impl Kernel {
    pub fn register_player_death_state(
        &mut self,
        actor: EntityId,
        state: PlayerDeathState,
    ) -> Result<(), (E, PlayerDeathState)> {
        let error = if state.validate().is_err() || self.characters.get(actor).is_none() {
            Some(E::Invalid)
        } else if self.player_deaths.states.contains_key(&actor) {
            Some(E::Stale)
        } else if self.player_deaths.states.len() >= 4096 {
            Some(E::Capacity)
        } else {
            None
        };
        if let Some(error) = error {
            return Err((error, state));
        }
        self.player_deaths.states.insert(actor, state);
        self.player_deaths.timers.insert(actor, self.tick);
        self.synchronize_death_projection(actor);
        Ok(())
    }
    pub fn player_death_state(&self, actor: EntityId) -> Option<&PlayerDeathState> {
        self.player_deaths.states.get(&actor)
    }
    /// Retained retry state is observable without consuming an output event.
    pub fn player_death_blocked(&self, actor: EntityId) -> Option<E> {
        self.player_deaths
            .pending
            .get(&actor)
            .and_then(|p| p.blocked)
    }
    pub fn player_death_pending(&self, actor: EntityId) -> bool {
        self.player_deaths.reserved(actor)
    }
    pub fn has_player_death_state(&self) -> bool {
        self.player_deaths.has_state()
    }
    pub fn prepare_player_vitae_recovery(
        &self,
        actor: EntityId,
        xp: u64,
    ) -> Result<Option<PlayerVitaeRecovery>, E> {
        self.prepare_player_vitae_recovery_after_item_xp(actor, xp, None)
    }
    pub fn prepare_player_vitae_recovery_after_item_xp(
        &self,
        actor: EntityId,
        xp: u64,
        item: Option<&crate::ItemExperienceRegistryChange>,
    ) -> Result<Option<PlayerVitaeRecovery>, E> {
        if self.player_deaths.reserved(actor) {
            return Err(E::Busy);
        }
        let Some(before) = self.player_deaths.states.get(&actor) else {
            return Err(E::MissingAssets);
        };
        let live = self.magic.registry(actor).ok_or(E::MissingAssets)?;
        let candidate;
        let registry = if let Some(item) = item {
            if item.actor != actor
                || item.before_revision != live.revision()
                || item.before != live.entries()
            {
                return Err(E::Stale);
            }
            candidate = bace_magic::EnchantmentRegistry::restore(
                item.capacity,
                item.after_revision,
                item.after.clone(),
            )
            .map_err(|_| E::Invalid)?;
            &candidate
        } else {
            live
        };
        let Some(entry) = registry
            .entries()
            .iter()
            .find(|entry| entry.spell == 666 && entry.spec.value < 1.)
        else {
            return Ok(None);
        };
        let change = bace_character::vitae_experience(
            entry.spec.value,
            before.vitae_pool,
            before.death_level,
            xp,
        )
        .map_err(|_| E::Invalid)?;
        let registry = registry
            .propose_vitae(
                actor.0,
                None,
                Some(change.after_value),
                change.remove_after_seconds,
            )
            .map_err(|_| E::Invalid)?;
        let mut after = before.clone();
        after.vitae_pool = change.after_pool;
        Ok(Some(PlayerVitaeRecovery {
            actor,
            before: before.clone(),
            after,
            before_enchantments: live.entries().to_vec(),
            registry,
        }))
    }
    pub fn validate_player_vitae_recovery(&self, patch: &PlayerVitaeRecovery) -> Result<(), E> {
        if self.player_deaths.states.get(&patch.actor) != Some(&patch.before)
            || self
                .magic
                .registry(patch.actor)
                .is_none_or(|r| r.revision() != patch.registry.before_revision())
        {
            return Err(E::Stale);
        }
        patch.after.validate()?;
        if !self.magic.can_accept() {
            return Err(E::Capacity);
        }
        Ok(())
    }
    /// The caller commits this supplement with the XP proposal before adoption.
    pub fn adopt_player_vitae_recovery(&mut self, patch: PlayerVitaeRecovery) -> Result<(), E> {
        self.validate_player_vitae_recovery(&patch)?;
        self.magic
            .adopt_vitae(patch.actor, patch.registry, self.tick as f64 / 30.)
            .map_err(|_| E::Busy)?;
        self.registry_revisions.insert(
            patch.actor,
            self.magic
                .registry(patch.actor)
                .ok_or(E::MissingAssets)?
                .revision(),
        );
        self.player_deaths.states.insert(patch.actor, patch.after);
        Ok(())
    }
    pub fn configure_player_death_random(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
        epoch: u64,
    ) -> Result<(), E> {
        if epoch == 0
            || self.player_deaths.random.is_some()
            || !self.player_deaths.pending.is_empty()
        {
            return Err(E::Invalid);
        }
        self.player_deaths.random = Some(root);
        self.player_deaths.epoch = epoch;
        Ok(())
    }
    pub fn take_player_death_event(&mut self) -> Option<crate::PlayerDeathEvent> {
        self.player_deaths.events.pop_front()
    }
}
impl Kernel {
    pub fn apply_player_death_command(
        &mut self,
        command: crate::PlayerDeathCommand,
    ) -> Result<(), (E, crate::PlayerDeathCommand)> {
        use crate::PlayerDeathCommand as C;
        match command {
            C::Prepare(input) => self
                .prepare_player_death(*input)
                .map_err(|(e, p)| (e, C::Prepare(p))),
            C::PrepareNoCorpse(input) => self
                .prepare_player_no_corpse(*input)
                .map_err(|(e, p)| (e, C::PrepareNoCorpse(p))),
            C::Committed {
                receipt,
                corpse_expiry_tick,
            } => self
                .confirm_player_death_committed_at(&receipt, corpse_expiry_tick)
                .map_err(|e| {
                    (
                        e,
                        C::Committed {
                            receipt,
                            corpse_expiry_tick,
                        },
                    )
                }),
            C::RegisterCorpseExpiry {
                corpse,
                death_operation,
                expires_tick,
            } => self
                .register_corpse_expiry(corpse, death_operation, expires_tick)
                .map_err(|e| {
                    (
                        e,
                        C::RegisterCorpseExpiry {
                            corpse,
                            death_operation,
                            expires_tick,
                        },
                    )
                }),
            C::ConfirmCorpseExpiry(receipt) => self
                .confirm_corpse_expiry_committed(&receipt)
                .map_err(|e| (e, C::ConfirmCorpseExpiry(receipt))),
            C::PrepareCorpseSpill(prepared) => self
                .prepare_corpse_spill(prepared)
                .map_err(|(e, p)| (e, C::PrepareCorpseSpill(p))),
            C::RetryCorpseExpiry { operation } => self
                .retry_corpse_expiry(operation)
                .map_err(|e| (e, C::RetryCorpseExpiry { operation })),
            C::Retry { operation } => self
                .retry_player_death(operation)
                .map_err(|e| (e, C::Retry { operation })),
        }
    }
}
impl Kernel {
    pub fn peek_player_death_event(&self) -> Option<&crate::PlayerDeathEvent> {
        self.player_deaths.events.front()
    }
    pub fn peek_player_death_proposal(&self) -> Option<&crate::PlayerDeathTicket> {
        self.player_deaths
            .pending
            .values()
            .find(|p| p.ticket.is_some() && !p.submitted && !p.committed)?
            .ticket
            .as_ref()
    }
}
#[cfg(test)]
mod no_corpse_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transcript_tests;

impl Kernel {
    pub fn validate_player_vitae_recoveries(
        &self,
        patches: &[PlayerVitaeRecovery],
    ) -> Result<(), E> {
        if patches.len() > 4096 {
            return Err(E::Capacity);
        }
        for patch in patches {
            self.validate_player_vitae_recovery(patch)?;
        }
        let registry: Vec<_> = patches.iter().map(|p| (p.actor, &p.registry)).collect();
        self.magic
            .validate_vitae_batch(&registry, self.tick as f64 / 30.)
            .map_err(|_| E::Capacity)
    }
}

impl Kernel {
    /// Validate the complete registry chain before the durable shared-XP operation.
    /// Adopt item changes first, then vitae; both consume this summed output budget.
    pub fn validate_player_vitae_recoveries_after_item_xp(
        &self,
        patches: &[PlayerVitaeRecovery],
        items: &[crate::ItemExperienceRegistryChange],
    ) -> Result<(), E> {
        if patches.len() > 4096 || items.len() > 4096 {
            return Err(E::Capacity);
        }
        self.magic
            .validate_item_experience_registries(items)
            .map_err(|_| E::Stale)?;
        for patch in patches {
            if self.player_deaths.states.get(&patch.actor) != Some(&patch.before) {
                return Err(E::Stale);
            }
            patch.after.validate()?;
        }
        let registry: Vec<_> = patches.iter().map(|p| (p.actor, &p.registry)).collect();
        self.magic
            .validate_vitae_batch_after(&registry, items, self.tick as f64 / 30.)
            .map_err(|_| E::Capacity)
    }
}

impl Kernel {
    pub fn take_player_death_service_outcome(
        &mut self,
    ) -> Option<crate::PlayerDeathServiceOutcome> {
        self.player_deaths.outcomes.pop_front()
    }
    /// Used only to restore a failed bounded publication. The sole output slot
    /// must still be empty: no command may run between take and restore.
    pub fn restore_player_death_service_outcome(
        &mut self,
        outcome: crate::PlayerDeathServiceOutcome,
    ) -> Result<(), Box<crate::PlayerDeathServiceOutcome>> {
        if !self.player_deaths.outcomes.is_empty() {
            return Err(Box::new(outcome));
        }
        self.player_deaths.outcomes.push_front(outcome);
        Ok(())
    }
}
