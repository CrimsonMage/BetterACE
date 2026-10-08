//! Single-owner kernel character operations.
use super::*;
impl Kernel {
    /// Transfer every player-owned domain state only after lifecycle durability
    /// has drained. Failure retains all owners inside the kernel.
    pub fn take_player_state(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<crate::OwnedPlayerState, CharacterRegistrationError> {
        if self
            .item_experience
            .events
            .iter()
            .any(|event| event.actor == binding.actor)
            || self.social.events.iter().any(|event|matches!(event,bace_gameplay_api::social::SocialEvent::EquipmentMana {recipient,..} if *recipient==binding.actor))
            || self.gag_pending(binding.actor)
            || self.equipment_mana_pending(binding.actor)
            || self.recall_busy(binding.actor)
            || self.staff.pending_for(binding.actor)
            || !self.world.can_retire_actor_motion(binding.actor)
            || self.world.health_observation_pending_for(binding.actor)
            || self.world.has_reserved_vitals(binding.actor)
            || self.pets.reserved(binding.actor)
            || self.pets.owner_has_active(binding.actor)
            || self.combat.active(binding.actor)
            || self.combat.dirty_involves(binding.actor)
            || self.npcs.pending_participant(binding.actor)
            || self.inventory.reserved(binding.actor)
            || self.housing.reserved(binding.actor)
            || self.magic.busy(binding.actor)
            || self.portals.reserved(binding.actor)
        {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        self.staff
            .can_remove(binding)
            .map_err(|_| CharacterRegistrationError::OwnershipMismatch)?;
        self.characters.can_take_complete(binding)?;
        if self.social.directory.presence(binding.actor).is_some() {
            self.can_take_social_presence(binding.actor)
                .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        }
        self.sync_social_age()
            .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        let chat_age = self.social_player_age(binding.actor);
        self.sync_player_world()
            .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        let world = Some(
            self.player_world_snapshot(binding.actor)
                .map_err(|_| CharacterRegistrationError::MissingActor)?,
        );
        let item_ids: Vec<_> = self
            .inventory
            .items()
            .filter(|item| {
                self.inventory.owned(binding.actor, item.id)
                    && self.magic.registry(item.id).is_some()
            })
            .map(|item| item.id)
            .collect();
        let now = self.tick as f64 / 30.0;
        for actor in item_ids
            .iter()
            .copied()
            .chain(std::iter::once(binding.actor))
            .filter(|actor| self.magic.registry(*actor).is_some())
            .collect::<Vec<_>>()
        {
            self.magic
                .can_take_registry(actor, now)
                .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        }
        let has_recovery = self.magic.recovery_revision(binding.actor).is_some();
        if has_recovery {
            self.magic
                .can_take_actor_recovery(binding.actor, now)
                .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        }
        self.sync_recovery_revisions()
            .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        self.sync_registry_revisions()
            .map_err(|_| CharacterRegistrationError::DurabilityPending)?;
        let recovery = if has_recovery {
            Some(
                self.magic
                    .take_actor_recovery(binding.actor, now)
                    .expect("single-owner recovery preflight"),
            )
        } else {
            None
        };
        self.world
            .retire_actor_motion(binding.actor)
            .expect("single-owner motion retirement preflight");
        self.recovery_revisions.remove(&binding.actor);
        self.physical_recovery_deadlines.remove(&binding.actor);
        let mut item_enchantments = Vec::with_capacity(item_ids.len());
        for id in item_ids {
            let registry = self
                .magic
                .take_registry(id, now)
                .expect("single-owner registry preflight");
            self.registry_revisions.remove(&id);
            item_enchantments.push((id, registry));
        }
        let enchantments = if self.magic.registry(binding.actor).is_some() {
            Some(
                self.magic
                    .take_registry(binding.actor, now)
                    .expect("single-owner registry preflight"),
            )
        } else {
            None
        };
        self.registry_revisions.remove(&binding.actor);
        let social = if self.social.directory.presence(binding.actor).is_some() {
            if self.fellowships.membership(binding.actor).is_some() {
                let now = u64::try_from(self.social_now().expect("social clock preflight"))
                    .expect("social nonnegative clock");
                self.remove_fellowship_member(binding.actor, false, now)
                    .expect("single-owner fellowship departure preflight");
            }
            Some(
                self.take_social_presence(binding.actor)
                    .expect("single-owner social preflight"),
            )
        } else {
            None
        };
        let character = self
            .characters
            .take_complete(binding)
            .expect("single-owner character preflight");
        self.world.clear_health_subscription(binding.actor);
        if self.staff.registered(binding.actor) {
            self.staff
                .remove(binding)
                .expect("accepted character/staff binding");
        }
        self.skill_devices.confirmations.remove(&binding.actor);
        self.attribute_transfers
            .confirmations
            .remove(&binding.actor);
        self.crafting.cancel_quote(binding.actor);
        self.avatar_locomotion.remove(&binding.actor);
        self.vital_inputs.remove(&binding.actor);
        self.combat.skills.remove(&binding.actor);
        let physical_recovery = self.capture_physical_recovery(binding.actor);
        self.combat.cancel(binding.actor);
        self.player_world_seen.remove(&binding.actor);
        self.player_world_dirty.remove(&binding.actor);
        let portal_links = self
            .take_portal_links(binding.actor)
            .expect("single-owner portal transfer preflight");
        let item_experience_ids: Vec<_> = self
            .item_experience
            .items
            .values()
            .filter(|p| p.actor == binding.actor)
            .map(|p| p.item)
            .collect();
        let item_experience = item_experience_ids
            .into_iter()
            .filter_map(|id| {
                let mut prepared = self.item_experience.items.remove(&id)?;
                if let Some(item) = self.inventory.item(id)
                    && let Some(xp) = &mut prepared.experience
                {
                    xp.revision = item.revision;
                }
                Some(prepared)
            })
            .collect();
        let equipment_mana = self.equipment_mana_snapshot(binding.actor);
        self.equipment_mana.players.remove(&binding.actor);
        Ok(crate::OwnedPlayerState {
            gag: self.social_gags.states.remove(&binding.actor),
            equipment_mana,
            chat_age,
            physical_recovery,
            item_experience,
            death: {
                self.player_deaths.timers.remove(&binding.actor);
                self.player_deaths.states.remove(&binding.actor)
            },
            social,
            portal_links,
            world,
            recovery,
            character,
            enchantments,
            item_enchantments,
        })
    }
    pub fn register_character_ui(
        &mut self,
        binding: CharacterBinding,
        state: crate::OwnedUiState,
    ) -> Result<(), (bace_gameplay_api::UiError, Box<crate::OwnedUiState>)> {
        self.characters.register_ui(binding, state)
    }
    pub fn character_ui(&self, actor: EntityId) -> Option<&bace_gameplay_api::CharacterUi> {
        self.characters.ui(actor)
    }
    pub fn character_entered(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<(), bace_gameplay_api::UiError> {
        self.characters.enter_ui(binding)?;
        self.world.finish_player_entry(binding.actor);
        Ok(())
    }
    pub fn apply_ui(
        &mut self,
        context: ActionContext,
        request: bace_gameplay_api::UiRequest,
    ) -> Result<u64, bace_gameplay_api::UiError> {
        if self.npcs.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.housing.reserved(context.actor)
        {
            return Err(bace_gameplay_api::UiError::DurabilityPending);
        }
        let revision =
            self.characters
                .apply_ui(context, request, self.world.body(context.actor).is_ok())?;
        if let Some(ui) = self.characters.ui(context.actor)
            && self.combat.physical_profile(context.actor).is_some()
        {
            self.combat
                .set_physical_options(context.actor, ui.options1, ui.options2)
                .map_err(|_| bace_gameplay_api::UiError::Invalid)?;
        }
        Ok(revision)
    }
    pub fn propose_skill(
        &mut self,
        context: ActionContext,
        intent: crate::SkillIntent,
    ) -> Result<crate::SkillTicket, crate::SkillActionError> {
        if self.npcs.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.housing.reserved(context.actor)
            || self.magic.busy(context.actor)
        {
            return Err(crate::SkillActionError::Busy);
        }
        if self.magic.registry_reserved(context.actor) {
            return Err(crate::SkillActionError::Busy);
        }
        let has_registry = self.magic.registry(context.actor).is_some();
        if has_registry {
            self.magic
                .reserve_registry(context.actor, true, self.tick as f64 / 30.0)
                .map_err(|_| crate::SkillActionError::Busy)?;
            if self.sync_registry_revisions().is_err() {
                let _ = self
                    .magic
                    .reserve_registry(context.actor, false, self.tick as f64 / 30.0);
                return Err(crate::SkillActionError::Capacity);
            }
        }
        let mut result =
            self.characters
                .propose_skill(context, intent, self.world.body(context.actor).is_ok());
        if let Ok(ticket) = result {
            let proposed = self.characters.proposed_skill_state(ticket)?;
            if self
                .validate_proposed_skill_refresh(context.actor, proposed)
                .is_err()
            {
                self.characters.reject_skill(ticket)?;
                result = Err(crate::SkillActionError::Profile);
            }
        }
        if result.is_err() && has_registry {
            self.magic
                .reserve_registry(context.actor, false, self.tick as f64 / 30.0)
                // The attempted action may already have consumed its sequence.
                // Busy is reserved for admission failures that can be retried.
                .map_err(|_| crate::SkillActionError::Profile)?;
        }
        result
    }
    pub fn confirm_skill_committed(
        &mut self,
        ticket: crate::SkillTicket,
    ) -> Result<bace_character::SkillTransitionChange, crate::SkillActionError> {
        if self
            .skill_devices
            .pending
            .values()
            .any(|device| device.skill == ticket)
        {
            return Err(crate::SkillActionError::Busy);
        }
        let proposed = self.characters.proposed_skill_state(ticket)?;
        self.validate_proposed_skill_refresh(ticket.context.actor, proposed)
            .map_err(|_| crate::SkillActionError::Profile)?;
        let change = self.characters.commit_skill(ticket)?;
        self.refresh_character_skills(ticket.context.actor)
            .map_err(|_| crate::SkillActionError::Profile)?;
        if self.magic.registry(ticket.context.actor).is_some() {
            self.magic
                .reserve_registry(ticket.context.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| crate::SkillActionError::Receipt)?;
        }
        Ok(change)
    }
    pub fn reject_skill(
        &mut self,
        ticket: crate::SkillTicket,
    ) -> Result<(), crate::SkillActionError> {
        if self
            .skill_devices
            .pending
            .values()
            .any(|device| device.skill == ticket)
        {
            return Err(crate::SkillActionError::Busy);
        }
        self.characters.reject_skill(ticket)?;
        if self.magic.registry(ticket.context.actor).is_some() {
            self.magic
                .reserve_registry(ticket.context.actor, false, self.tick as f64 / 30.0)
                .map_err(|_| crate::SkillActionError::Receipt)?;
        }
        Ok(())
    }
    pub fn take_skill_outcome(&mut self) -> Option<crate::SkillOutcome> {
        self.skill_outcomes.pop_front()
    }
    pub fn take_ui_outcome(&mut self) -> Option<crate::UiOutcome> {
        self.ui_outcomes.pop_front()
    }
    pub fn has_character_feature_work(&self) -> bool {
        !self.crafting_outcomes.is_empty()
            || !self.skill_device_outcomes.is_empty()
            || self.has_skill_device_work()
            || !self.attribute_transfer_outcomes.is_empty()
            || self.has_attribute_transfer_work()
            || !self.skill_outcomes.is_empty()
            || !self.ui_outcomes.is_empty()
            || self.commands.iter().any(|c| {
                matches!(
                    c,
                    Command::Crafting(_)
                        | Command::SkillDevice(_)
                        | Command::AttributeTransfer(_)
                        | Command::Ui { .. }
                        | Command::TrainSkill { .. }
                        | Command::CommitSkill { .. }
                        | Command::RollbackSkill { .. }
                )
            })
    }
    pub fn register_character_with_rare(
        &mut self,
        binding: CharacterBinding,
        state: crate::OwnedCharacterState,
    ) -> Result<(), (CharacterRegistrationError, Box<crate::OwnedCharacterState>)> {
        if self.world.body(binding.actor).is_err() {
            return Err((CharacterRegistrationError::MissingActor, Box::new(state)));
        }
        if self
            .world
            .combatant(binding.actor)
            .filter(|c| c.profile().player)
            .is_some_and(|c| c.validate_incarnation(binding.session.0).is_err())
        {
            return Err((
                CharacterRegistrationError::OwnershipMismatch,
                Box::new(state),
            ));
        }
        self.characters.register_complete(binding, state)?;
        if let Some(combatant) = self
            .world
            .combatant_mut(binding.actor)
            .filter(|c| c.profile().player)
        {
            combatant
                .stamp_incarnation(binding.session.0)
                .expect("same-owner lifetime preflight");
        }
        let snapshot = self
            .player_world_snapshot(binding.actor)
            .expect("registered actor has accepted world state");
        self.player_world_seen.insert(binding.actor, snapshot);
        Ok(())
    }
    pub fn take_character_with_rare(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<crate::OwnedCharacterState, CharacterRegistrationError> {
        self.characters.can_take_complete(binding)?;
        if self
            .item_experience
            .items
            .values()
            .any(|p| p.actor == binding.actor)
            || self.social_gags.states.contains_key(&binding.actor)
            || self.equipment_mana.players.contains_key(&binding.actor)
            || self.magic.registry(binding.actor).is_some()
            || self.magic.recovery_revision(binding.actor).is_some()
            || self.portal_links(binding.actor).is_some()
            || self.needs_world_state_transfer(binding.actor)
        {
            return Err(CharacterRegistrationError::CompleteStateRequired);
        }

        if !self.world.can_retire_actor_motion(binding.actor)
            || self.pets.reserved(binding.actor)
            || self.world.has_reserved_vitals(binding.actor)
            || self.pets.owner_has_active(binding.actor)
            || self.physical_recovery_pending(binding.actor)
            || self.combat.dirty_involves(binding.actor)
            || self.npcs.pending_participant(binding.actor)
            || self.inventory.reserved(binding.actor)
            || self.housing.reserved(binding.actor)
            || self.magic.busy(binding.actor)
            || self.portals.reserved(binding.actor)
        {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        let state = self.characters.take_complete(binding)?;
        self.world
            .retire_actor_motion(binding.actor)
            .expect("single-owner motion retirement preflight");
        self.skill_devices.confirmations.remove(&binding.actor);
        self.attribute_transfers
            .confirmations
            .remove(&binding.actor);
        self.crafting.cancel_quote(binding.actor);
        self.avatar_locomotion.remove(&binding.actor);
        self.vital_inputs.remove(&binding.actor);
        self.combat.skills.remove(&binding.actor);
        self.combat.cancel(binding.actor);
        self.player_world_seen.remove(&binding.actor);
        self.player_world_dirty.remove(&binding.actor);
        Ok(state)
    }
    pub fn character_rare_state(
        &self,
        actor: EntityId,
    ) -> Option<bace_gameplay_api::CharacterRareState> {
        self.characters.rare(actor)
    }
    pub fn register_character(
        &mut self,
        binding: CharacterBinding,
        progression: CharacterProgression,
    ) -> Result<(), (CharacterRegistrationError, CharacterProgression)> {
        if self.world.body(binding.actor).is_err() {
            return Err((CharacterRegistrationError::MissingActor, progression));
        }
        if self
            .world
            .combatant(binding.actor)
            .filter(|c| c.profile().player)
            .is_some_and(|c| c.validate_incarnation(binding.session.0).is_err())
        {
            return Err((CharacterRegistrationError::OwnershipMismatch, progression));
        }
        self.characters.register(binding, progression)?;
        if let Some(combatant) = self
            .world
            .combatant_mut(binding.actor)
            .filter(|c| c.profile().player)
        {
            combatant
                .stamp_incarnation(binding.session.0)
                .expect("same-owner lifetime preflight");
        }
        let snapshot = self
            .player_world_snapshot(binding.actor)
            .expect("registered actor has accepted world state");
        self.player_world_seen.insert(binding.actor, snapshot);
        Ok(())
    }
    pub fn take_character(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<CharacterProgression, CharacterRegistrationError> {
        self.characters.can_take_complete(binding)?;
        if self
            .item_experience
            .items
            .values()
            .any(|p| p.actor == binding.actor)
            || self.social_gags.states.contains_key(&binding.actor)
            || self.equipment_mana.players.contains_key(&binding.actor)
            || self.magic.registry(binding.actor).is_some()
            || self.magic.recovery_revision(binding.actor).is_some()
            || self.portal_links(binding.actor).is_some()
            || self.needs_world_state_transfer(binding.actor)
        {
            return Err(CharacterRegistrationError::CompleteStateRequired);
        }

        if !self.world.can_retire_actor_motion(binding.actor)
            || self.pets.reserved(binding.actor)
            || self.pets.owner_has_active(binding.actor)
            || self.world.has_reserved_vitals(binding.actor)
            || self.physical_recovery_pending(binding.actor)
            || self.combat.dirty_involves(binding.actor)
            || self.npcs.pending_participant(binding.actor)
            || self.inventory.reserved(binding.actor)
            || self.housing.reserved(binding.actor)
            || self.magic.busy(binding.actor)
            || self.portals.reserved(binding.actor)
        {
            return Err(CharacterRegistrationError::DurabilityPending);
        }
        let character = self.characters.take(binding)?;
        self.world
            .retire_actor_motion(binding.actor)
            .expect("single-owner motion retirement preflight");
        self.skill_devices.confirmations.remove(&binding.actor);
        self.attribute_transfers
            .confirmations
            .remove(&binding.actor);
        self.crafting.cancel_quote(binding.actor);
        self.avatar_locomotion.remove(&binding.actor);
        self.vital_inputs.remove(&binding.actor);
        self.combat.skills.remove(&binding.actor);
        self.combat.cancel(binding.actor);
        self.player_world_seen.remove(&binding.actor);
        self.player_world_dirty.remove(&binding.actor);
        Ok(character)
    }
    pub fn character(&self, actor: EntityId) -> Option<&CharacterProgression> {
        self.characters.get(actor)
    }
    pub fn has_characters(&self) -> bool {
        !self.characters.is_empty()
    }
}
