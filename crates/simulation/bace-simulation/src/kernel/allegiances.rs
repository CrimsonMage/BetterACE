//! Allegiance authority and receipt-gated metadata transitions.
use super::Kernel;
use bace_allegiance::{AllegianceManagement, AllegianceRegistry};
use bace_gameplay_api::social::{AllegianceRelation, AllegianceSanctuary, SocialError};
use bace_types::EntityId;
impl Kernel {
    pub fn allegiance_relation(&self, actor: EntityId) -> Option<AllegianceRelation> {
        let registry = &self.allegiances.registry;
        let node = registry.node(actor)?;
        Some(AllegianceRelation {
            character: actor,
            monarch: node.monarch,
            patron: node.patron,
            rank: node.rank,
            officer_level: registry.permission(actor),
            revision: registry.revision(),
        })
    }
    pub fn allegiance_hometown(
        &self,
        actor: EntityId,
    ) -> Option<(EntityId, u64, AllegianceSanctuary)> {
        let relation = self.allegiance_relation(actor)?;
        let sanctuary = self
            .allegiances
            .registry
            .metadata(relation.monarch)?
            .sanctuary?;
        Some((relation.monarch, relation.revision, sanctuary))
    }
    pub fn register_allegiances(
        &mut self,
        registry: AllegianceRegistry,
        now_seconds: f64,
    ) -> Result<(), SocialError> {
        if self.allegiances.has_state() {
            return Err(SocialError::Busy);
        }
        let clock = bace_allegiance::AllegianceClock::new(now_seconds)?;
        self.allegiances.registry = registry;
        self.allegiances.clock = clock;
        Ok(())
    }
    pub fn prepare_allegiance_sanctuary(
        &mut self,
        actor: EntityId,
        position: AllegianceSanctuary,
    ) -> Result<crate::allegiances::AllegianceTicket, SocialError> {
        self.prepare_allegiance_management(actor, AllegianceManagement::Sanctuary(position))
    }
    pub fn prepare_allegiance_management(
        &mut self,
        actor: EntityId,
        request: AllegianceManagement,
    ) -> Result<crate::allegiances::AllegianceTicket, SocialError> {
        let patch = self
            .allegiances
            .registry
            .propose_management(actor, request)?;
        self.allegiances.propose(actor, patch, Vec::new())
    }
    pub fn take_allegiance_proposal(&mut self) -> Option<crate::allegiances::AllegianceTicket> {
        self.allegiances.take()
    }
    pub fn confirm_allegiance_committed(
        &mut self,
        ticket: &crate::allegiances::AllegianceTicket,
    ) -> Result<(), SocialError> {
        if self.allegiances.pve_operation.is_some() {
            return Err(SocialError::Busy);
        }
        self.validate_allegiance_commit(ticket)?;
        self.adopt_allegiance_commit(ticket)
    }
    pub(super) fn validate_allegiance_commit(
        &self,
        ticket: &crate::AllegianceTicket,
    ) -> Result<(), SocialError> {
        self.allegiances.validate_receipt(ticket)?;
        self.validate_staff_reward(ticket, true)?;
        self.validate_experience_events(ticket)?;
        if let Some(npc) = &ticket.npc {
            self.npcs
                .validate_shared_experience(npc)
                .map_err(|_| SocialError::Stale)?;
        }
        if !ticket.player_changes.is_empty() {
            let table = self
                .allegiances
                .level_table
                .as_ref()
                .ok_or(SocialError::Missing)?;
            self.characters
                .validate_social_rewards(ticket.operation, &ticket.player_changes, table)
                .map_err(|_| SocialError::Stale)?;
        }
        if let Some(rare) = &ticket.rare {
            self.characters
                .validate_social_rare(rare)
                .map_err(|_| SocialError::Stale)?;
            if !ticket
                .player_changes
                .iter()
                .any(|(id, _)| id.0 == rare.character)
            {
                return Err(SocialError::Invalid);
            }
        }
        if !ticket.vitals.is_empty() {
            self.world
                .validate_vital_batch_reserved(
                    &ticket.vitals,
                    None,
                    super::reward_supplements::experience_vital_token(ticket.operation),
                )
                .map_err(|_| SocialError::Stale)?;
        }
        self.validate_item_experience_events_batch(
            ticket.item_experience.iter().map(|(reward, _)| reward),
        )
        .map_err(|_| SocialError::Capacity)?;
        for (reward, reservation) in &ticket.item_experience {
            self.validate_item_experience(reward, reservation.as_ref())
                .map_err(|_| SocialError::Stale)?;
        }
        let patches = ticket
            .item_experience
            .iter()
            .flat_map(|(r, _)| r.registries.clone())
            .collect::<Vec<_>>();
        self.validate_player_vitae_recoveries_after_item_xp(&ticket.vitae, &patches)
            .map_err(|_| SocialError::Capacity)?;
        if self.social.events.len().saturating_add(1) > self.social.capacity
            || self.social.outcomes.len() >= self.social.capacity
        {
            return Err(SocialError::Capacity);
        }
        Ok(())
    }
    pub(super) fn adopt_allegiance_commit(
        &mut self,
        ticket: &crate::AllegianceTicket,
    ) -> Result<(), SocialError> {
        if !ticket.player_changes.is_empty() {
            let table = self
                .allegiances
                .level_table
                .as_ref()
                .ok_or(SocialError::Missing)?;
            self.characters
                .adopt_social_rewards(ticket.operation, &ticket.player_changes, table)
                .map_err(|_| SocialError::Stale)?;
        }
        if let Some(rare) = &ticket.rare {
            let (_, change) = ticket
                .player_changes
                .iter()
                .find(|(id, _)| id.0 == rare.character)
                .ok_or(SocialError::Invalid)?;
            self.characters
                .adopt_social_rare(rare, &change.experience)
                .map_err(|_| SocialError::Stale)?;
        }
        for (reward, reservation) in &ticket.item_experience {
            self.adopt_item_experience(reward, reservation.as_ref())
                .map_err(|_| SocialError::Stale)?;
        }
        for patch in &ticket.vitae {
            self.adopt_player_vitae_recovery(patch.clone())
                .map_err(|_| SocialError::Stale)?;
        }
        if !ticket.vitals.is_empty() {
            self.world
                .apply_vital_batch_reserved(
                    &ticket.vitals,
                    None,
                    super::reward_supplements::experience_vital_token(ticket.operation),
                )
                .map_err(|_| SocialError::Stale)?;
            for v in &ticket.vitals {
                if let Some(seen) = self.player_world_seen.get_mut(&v.actor) {
                    let index = match v.vital {
                        bace_entity::EntityVital::Health => 0,
                        bace_entity::EntityVital::Stamina => 1,
                        bace_entity::EntityVital::Mana => 2,
                    };
                    if let Some(pool) = &mut seen.vitals[index] {
                        pool.current = v.after;
                    }
                }
            }
            self.world
                .release_vitals(super::reward_supplements::experience_vital_token(
                    ticket.operation,
                ));
        }
        for (actor, _) in &ticket.player_changes {
            self.characters
                .adopt_social_revision(
                    *actor,
                    ticket
                        .player_revision(*actor)
                        .ok_or(SocialError::Overflow)?,
                )
                .map_err(|_| SocialError::Stale)?;
        }
        if let Some(npc) = &ticket.npc {
            self.npcs
                .mark_shared_experience_adopted(npc)
                .map_err(|_| SocialError::Stale)?;
            self.commit_npc_source_inventory(npc)
                .map_err(|_| SocialError::Stale)?;
        }
        self.allegiances.adopt(ticket)?;
        self.emit_experience_events(ticket);
        self.release_social_registries()?;
        if let Some(context) = self.allegiances.context.take() {
            self.social
                .outcomes
                .push_back(bace_gameplay_api::social::SocialOutcome {
                    context,
                    retryable: false,
                    result: Ok(()),
                });
        }
        if self
            .social
            .directory
            .presence(ticket.actor)
            .is_some_and(|p| p.online)
        {
            self.emit_allegiance(ticket.actor)?;
        }
        self.complete_staff_reward(ticket, true);
        Ok(())
    }
    pub fn reject_allegiance_proposal(
        &mut self,
        ticket: &crate::allegiances::AllegianceTicket,
    ) -> Result<(), SocialError> {
        if self.allegiances.pve_operation.is_some() || ticket.npc.is_some() {
            return Err(SocialError::Busy);
        }
        self.allegiances.validate_receipt(ticket)?;
        self.validate_staff_reward(ticket, false)?;
        if self.social.outcomes.len() >= self.social.capacity {
            return Err(SocialError::Capacity);
        }
        if let Some(context) = self.allegiances.context.take() {
            self.social
                .outcomes
                .push_back(bace_gameplay_api::social::SocialOutcome {
                    context,
                    retryable: false,
                    result: Err(SocialError::Stale),
                });
        }
        self.world
            .release_vitals(super::reward_supplements::experience_vital_token(
                ticket.operation,
            ));
        for (_, reservation) in &ticket.item_experience {
            self.reject_item_experience(reservation.as_ref())
                .map_err(|_| SocialError::Stale)?;
        }
        self.characters
            .reject_social_rewards(ticket.operation, &ticket.player_changes)
            .map_err(|_| SocialError::Stale)?;
        self.release_social_registries()?;
        self.allegiances.pending = None;
        self.allegiances.submitted = false;
        self.allegiances.checkpoint = None;
        self.allegiances.cache_operation = None;
        self.complete_staff_reward(ticket, false);
        Ok(())
    }
}
