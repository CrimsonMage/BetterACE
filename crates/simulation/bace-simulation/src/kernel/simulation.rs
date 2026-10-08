//! Single-owner kernel simulation operations.
use super::*;
impl Kernel {
    pub fn take_crafting_outcome(&mut self) -> Option<crate::CraftingOutcome> {
        self.crafting_outcomes.pop_front()
    }

    pub fn native_loot_failure(&self) -> Option<PveError> {
        self.population.native_error()
    }

    pub fn supply_projectile_id(
        &mut self,
        id: EntityId,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        if (self.population.reserves_identity(id) || self.generator_reserves_identity(id))
            || self.inventory.item(id).is_some()
            || self.inventory.reserved(id)
        {
            return Err(bace_gameplay_api::CastRejection::InvalidState);
        }
        self.magic.supply_projectile_id(id, &self.world)
    }
    pub fn configure_native_loot(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
        epoch: u64,
    ) -> Result<(), PveError> {
        self.population.configure_native_loot(root, epoch)
    }
    pub fn register_native_loot(
        &mut self,
        actor: EntityId,
        policy: crate::NativeLootPolicy,
        event: [u8; 16],
    ) -> Result<(), PveError> {
        self.population.native_loot(actor, policy, event)
    }
    pub fn supply_native_spawn_id(
        &mut self,
        id: EntityId,
        event: [u8; 16],
    ) -> Result<(), PveError> {
        if self.magic.reserves_identity(id)
            || self.inventory.reserved(id)
            || self.inventory.item(id).is_some()
        {
            return Err(PveError::Duplicate);
        }
        self.population.native_spawn_id(id, event, &self.world)
    }
    pub fn take_progression_outcome(&mut self) -> Option<ProgressionOutcome> {
        self.progression_outcomes.pop_front()
    }
    pub fn take_combat_outcome(&mut self) -> Option<CombatOutcome> {
        self.combat_outcomes.pop_front()
    }
    pub fn take_combat_event(&mut self) -> Option<CombatEvent> {
        self.combat_events.pop_front()
    }
    pub fn pending_combat_outcomes(&self) -> usize {
        self.combat_outcomes.len()
    }
    pub fn pending_combat_events(&self) -> usize {
        self.combat.pending_events()
            + self.combat_events.len()
            + usize::from(self.combat.has_dirty())
    }
    pub fn supply_loot_random(&mut self, draws: &[f32]) -> Result<(), PveError> {
        self.population.feed_random(draws)
    }
    pub fn supply_spawn_id(&mut self, id: EntityId) -> Result<(), PveError> {
        if self.magic.reserves_identity(id)
            || self.inventory.reserved(id)
            || self.inventory.item(id).is_some()
        {
            return Err(PveError::Duplicate);
        }
        self.population.feed_id(id, &self.world)
    }
    pub fn take_death_proposal(&mut self) -> Option<DeathProposal> {
        let proposal = self.population.take_proposal()?;
        if proposal.social.is_some() {
            self.allegiances.submitted = true;
        }
        Some(proposal)
    }
    pub fn take_pve_event(&mut self) -> Option<PveEvent> {
        self.population.take_event()
    }
    pub fn pending_deaths(&self) -> usize {
        self.population.pending()
    }
    pub fn retry_death(&mut self, operation: u64) -> Result<(), PveError> {
        self.population.retry(operation)
    }
    pub fn confirm_death_committed(
        &mut self,
        operation: u64,
        corpse: EntityId,
        items: &[EntityId],
        persisted_credits: &[(EntityId, bace_character::ExperienceCredit)],
    ) -> Result<(), PveError> {
        if self.population.shared_death_ticket(operation).is_some() {
            return Err(PveError::InvalidReceipt);
        }
        self.population.committed(
            operation,
            corpse,
            items,
            persisted_credits,
            &mut self.world,
            self.tick,
        )?;
        self.characters
            .commit_rewards(operation)
            .map_err(|()| PveError::InvalidReceipt)
    }
    pub fn register_door(
        &mut self,
        id: EntityId,
        prepared: PreparedDoor,
    ) -> Result<(), DoorRejection> {
        if (self.population.reserves_identity(id) || self.generator_reserves_identity(id))
            || self.magic.reserves_identity(id)
            || self.inventory.reserved(id)
            || self.inventory.item(id).is_some()
        {
            return Err(DoorRejection::InvalidState);
        }
        self.doors.register(id, prepared, &mut self.world)
    }
    pub fn take_door_outcome(&mut self) -> Option<DoorOutcome> {
        self.door_outcomes.pop_front()
    }
    pub fn take_door_event(&mut self) -> Option<DoorEvent> {
        self.doors.take_event()
    }
    pub fn pending_door_outcomes(&self) -> usize {
        self.door_outcomes.len()
    }
    pub fn pending_door_events(&self) -> usize {
        self.doors.pending_events()
    }
    pub fn has_door_state(&self) -> bool {
        self.doors.has_state()
            || !self.door_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::UseDoor { .. }))
    }
    pub fn door_physics(&self, id: EntityId) -> Option<bace_interactions::DoorPhysics> {
        self.doors.physics(id)
    }
    pub fn has_pve_state(&self) -> bool {
        self.population.has_state()
            || !self.pve_service_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::PveService(_)))
    }
    pub fn pending_progression_outcomes(&self) -> usize {
        self.progression_outcomes.len()
    }
    pub fn queued_commands(&self) -> usize {
        self.commands.len()
    }
    pub fn has_queued_progression(&self) -> bool {
        self.commands
            .iter()
            .any(|command| matches!(command, Command::RaiseProgression { .. }))
    }
    pub fn has_queued_combat(&self) -> bool {
        self.commands
            .iter()
            .any(|command| matches!(command, Command::Combat { .. }))
    }
    pub fn has_queued_door(&self) -> bool {
        self.commands
            .iter()
            .any(|command| matches!(command, Command::UseDoor { .. }))
    }
    pub fn progression_backpressured(&self) -> bool {
        self.progression_outcomes.len() == self.outcome_capacity
            && matches!(
                self.commands.front(),
                Some(Command::RaiseProgression { .. })
            )
    }
    pub fn confirm_corpse_item_removed(&mut self, corpse: EntityId, item: EntityId) -> bool {
        self.world.confirm_corpse_item_removed(corpse, item)
    }
    // Backpressure retries retain the bounded inline command without allocating
    // and freeing a box on each tick while the queue remains full.
    #[expect(
        clippy::result_large_err,
        reason = "return queued command ownership without per-retry allocation"
    )]
    pub fn try_enqueue(&mut self, command: Command) -> Result<(), Command> {
        if self.commands.len() >= self.capacity
            || matches!(&command, Command::DetachPlayer(c) if !c.valid_bounds())
            || matches!(&command, Command::Crafting(c) if c.validate_bounds().is_err())
            || matches!(&command, Command::Inventory(c) if !c.valid_bounds())
            || matches!(&command,Command::SkillDevice(c) if !c.valid_bounds())
            || matches!(&command,Command::AttributeTransfer(c) if !c.valid_bounds())
            || matches!(&command,Command::PortalResolution(c) if !c.valid_bounds())
            || matches!(&command, Command::Generator(c) if !c.valid_bounds())
            || matches!(&command, Command::Vendor(c) if !c.valid_bounds())
            || matches!(&command,Command::PlayerDeath(c) if !c.valid_bounds())
            || matches!(&command,Command::PlayerDeathService(c) if !c.valid_bounds())
            || matches!(&command,Command::CorpseAccess(c) if !c.valid_bounds())
            || matches!(&command,Command::CorpseConsent(c) if !c.valid_bounds())
            || matches!(&command,Command::Pet(c) if !c.valid_bounds())
            || matches!(&command,Command::NpcService(c) if !c.valid_bounds())
            || matches!(&command,Command::Visibility(c) if !c.valid_bounds())
            || matches!(&command,Command::ObjectView(c) if !c.valid_bounds())
            || matches!(&command,Command::PhysicalResource(c) if !c.valid_bounds())
            || matches!(&command,Command::MagicResource(c) if !c.valid_bounds())
        {
            return Err(command);
        }
        self.commands.push_back(command);
        Ok(())
    }
    pub fn enqueue(&mut self, command: Command) -> Result<(), SimulationError> {
        if matches!(&command, Command::Crafting(c) if c.validate_bounds().is_err())
            || matches!(&command, Command::Inventory(c) if !c.valid_bounds())
            || matches!(&command,Command::SkillDevice(c) if !c.valid_bounds())
            || matches!(&command,Command::AttributeTransfer(c) if !c.valid_bounds())
            || matches!(&command,Command::PortalResolution(c) if !c.valid_bounds())
            || matches!(&command, Command::DetachPlayer(c) if !c.valid_bounds())
            || matches!(&command,Command::Generator(c) if !c.valid_bounds())
            || matches!(&command,Command::Vendor(c) if !c.valid_bounds())
            || matches!(&command,Command::PlayerDeath(c) if !c.valid_bounds())
            || matches!(&command,Command::PlayerDeathService(c) if !c.valid_bounds())
            || matches!(&command,Command::CorpseAccess(c) if !c.valid_bounds())
            || matches!(&command,Command::CorpseConsent(c) if !c.valid_bounds())
            || matches!(&command,Command::Pet(c) if !c.valid_bounds())
            || matches!(&command,Command::NpcService(c) if !c.valid_bounds())
            || matches!(&command,Command::Visibility(c) if !c.valid_bounds())
            || matches!(&command,Command::ObjectView(c) if !c.valid_bounds())
            || matches!(&command,Command::PhysicalResource(c) if !c.valid_bounds())
            || matches!(&command,Command::MagicResource(c) if !c.valid_bounds())
        {
            return Err(SimulationError::InvalidCommand);
        }
        if self.commands.len() >= self.capacity {
            return Err(SimulationError::QueueFull);
        }
        self.commands.push_back(command);
        Ok(())
    }
    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn ticks(&self) -> u64 {
        self.tick
    }
    fn ingest_combat_event(&mut self, event: CombatEvent, tick: u64) -> bool {
        if matches!(event, CombatEvent::Damage {target,killed:true,..} if self.combat.physical_proc_pending(target))
        {
            return false;
        }
        if let CombatEvent::Damage {
            attacker,
            death_blow,
            target,
            killed: true,
            ..
        } = event
            && self.characters.get(target).is_some()
        {
            return self
                .begin_player_death_with_blow(target, attacker, death_blow)
                .is_ok();
        }
        if let CombatEvent::Damage {
            target,
            killed: true,
            ..
        } = event
            && self.world.combatant(target).is_some_and(|state| {
                state.contributors().iter().any(|(actor, _)| {
                    self.inventory.reserved(*actor)
                        || self.npcs.reserved(*actor)
                        || self.housing.reserved(*actor)
                })
            })
        {
            return false;
        }
        if let CombatEvent::Damage {
            target,
            killed: true,
            ..
        } = event
            && self
                .population
                .refresh_owned_loot(target, &self.inventory)
                .is_err()
        {
            return false;
        }
        let suppress = match event {
            CombatEvent::Damage {
                target,
                killed: true,
                ..
            } => self
                .world
                .actor_state(target)
                .is_ok_and(|(cell, _)| self.world_policies.suppresses_kill_experience(cell.0)),
            _ => false,
        };
        let social = &self.social.directory;
        self.population.ingest(
            event,
            &self.world,
            &mut self.characters,
            tick,
            (self.allegiances.level_table.is_some(), suppress),
            |id| social.presence(id).is_some_and(|presence| presence.olthoi),
        )
    }
    pub fn step(&mut self) -> Result<Vec<String>, SimulationError> {
        let next_tick = self
            .tick
            .checked_add(1)
            .ok_or(SimulationError::TickOverflow)?;
        self.combat.set_simulation_tick(next_tick);
        let mut rejected = Vec::new();
        self.step_player_deaths(next_tick)?;
        self.drain_cast_outcomes();
        self.step_equipment_mana(next_tick as f64 / 30.)?;
        self.step_social_gags()?;
        self.refresh_changed_magic_skills()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.refresh_changed_physical_qualities()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.refresh_dirty_locomotion()?;
        while !self.commands.is_empty() {
            if matches!(self.commands.front(), Some(Command::PlayerDeathService(_)))
                && !self.player_deaths.outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::CorpseAccess(_)))
                && self.player_deaths.corpse_access_outcomes.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::CorpseConsent(_)))
                && self.player_deaths.consent_outcomes.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Pet(_)))
                && self.pet_outcomes.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::PortalResolution(_)))
                && self.portals.resolutions.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::AdmitResidentRegion(_)))
                && !self.region_admission_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::PlayerEntered(_)))
                && !self.player_entered_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::DetachPlayer(_)))
                && !self.player_detach_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::AdmitPlayer(_)))
                && !self.player_admission_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::PlayerSnapshot(_)))
                && !self.player_snapshot_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::PveService(_)))
                && self.pve_service_outcomes.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Visibility(_)))
                && !self.visibility_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::ObjectView(_)))
                && !self.object_view_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Locomotion(_)))
                && !self.locomotion_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::PhysicalResource(_)))
                && !self.physical_resource_outcomes.is_empty()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::MagicResource(_)))
                && self.magic_resources.outcomes.len() >= self.magic_resources.capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::NpcService(_)))
                && self.npc_service_outcomes.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::SocialControl(_)))
                && (self.social.controls.len() >= self.social.capacity
                    || self.social.events.len().saturating_add(4) > self.social.capacity
                    || self.social.outcomes.len() >= self.social.capacity)
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Allegiance { .. }))
                && self.allegiances.pending.is_some()
            {
                break;
            }
            if matches!(
                self.commands.front(),
                Some(
                    Command::Social { .. }
                        | Command::SocialResolved { .. }
                        | Command::Fellowship { .. }
                        | Command::Allegiance { .. }
                )
            ) && (self.social.outcomes.len() >= self.social.capacity
                || self.social.events.len().saturating_add(4) > self.social.capacity)
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Recall(_)))
                && self.recalls.events.len() >= self.recalls.capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Generator(_)))
                && self.generator_outcomes.len() == self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Vendor(_)))
                && self.vendor_outcomes.len() >= self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Inventory(_)))
                && self.inventory_commands_backpressured()
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Crafting(_)))
                && self.crafting_outcomes.len() == self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::SkillDevice(_)))
                && self.skill_device_outcomes.len() == self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::AttributeTransfer(_)))
                && self.attribute_transfer_outcomes.len() == self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Ui { .. }))
                && self.ui_outcomes.len() == self.outcome_capacity
            {
                break;
            }
            if matches!(
                self.commands.front(),
                Some(
                    Command::TrainSkill { .. }
                        | Command::CommitSkill { .. }
                        | Command::RollbackSkill { .. }
                )
            ) && self.skill_outcomes.len() == self.outcome_capacity
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::UseDoor { .. }))
                && (self.door_outcomes.len() == self.outcome_capacity || !self.doors.can_accept())
            {
                break;
            }
            if matches!(self.commands.front(), Some(Command::Cast { .. }))
                && (self.cast_outcomes.len() == self.outcome_capacity || !self.magic.can_accept())
            {
                break;
            }

            // Preserve FIFO and retain the request, with no mutation or consumed
            // sequence, until its correlated result has bounded output capacity.
            if self.progression_backpressured()
                || (self.combat_outcomes.len() == self.outcome_capacity
                    && matches!(self.commands.front(), Some(Command::Combat { .. })))
            {
                break;
            }
            if matches!(self.commands.front(),Some(Command::RaiseProgression{context,..}) if self.characters.reserved(context.actor)||self.npcs.reserved(context.actor)||self.inventory.reserved(context.actor)||self.housing.reserved(context.actor))
            {
                let Some(Command::RaiseProgression { context, .. }) = self.commands.pop_front()
                else {
                    unreachable!("matched progression front")
                };
                self.progression_outcomes.push_back(ActionResult {
                    context,
                    result: Err(ProgressionActionRejection::DurabilityPending),
                });
                continue;
            }
            let command = self.commands.pop_front().expect("nonempty queue");
            let result = match command {
                Command::ActorMagicProgram {
                    correlation,
                    program,
                } => {
                    if let Err(program) = self.apply_actor_magic_program(correlation, *program) {
                        self.commands.push_front(Command::ActorMagicProgram {
                            correlation,
                            program,
                        });
                        break;
                    }
                    Ok(())
                }
                Command::Staff(command) => {
                    if let Err(command) = self.apply_staff_command(command) {
                        self.commands.push_front(Command::Staff(*command));
                        break;
                    }
                    Ok(())
                }
                Command::SocialControl(command) => {
                    self.apply_social_control(command);
                    Ok(())
                }
                Command::Pet(command) => {
                    self.handle_pet_command(command);
                    Ok(())
                }
                Command::Allegiance { context, request } => {
                    let _ = self.request_allegiance(context, request);
                    Ok(())
                }
                Command::Fellowship { context, request } => {
                    let _ = self.request_fellowship(context, request);
                    Ok(())
                }
                Command::SocialResolved {
                    context,
                    request,
                    identity,
                } => {
                    let _ = self.request_social_resolved(context, request, identity);
                    Ok(())
                }
                Command::Social { context, request } => {
                    let _ = self.request_social(context, request);
                    Ok(())
                }
                Command::AdmitResidentRegion(request) => {
                    self.handle_region_admission(request);
                    Ok(())
                }
                Command::PlayerEntered(request) => {
                    self.handle_player_entered(request);
                    Ok(())
                }
                Command::DetachPlayer(request) => {
                    self.handle_player_detach(request);
                    Ok(())
                }
                Command::AdmitPlayer(request) => {
                    self.handle_player_admission(request);
                    Ok(())
                }
                Command::PlayerSnapshot(request) => {
                    self.handle_player_snapshot(request);
                    Ok(())
                }
                Command::PveService(command) => {
                    self.handle_pve_service(command);
                    Ok(())
                }
                Command::NpcService(command) => {
                    self.handle_npc_service(*command);
                    Ok(())
                }
                Command::Visibility(request) => {
                    self.handle_visibility_request(*request);
                    Ok(())
                }
                Command::ObjectView(request) => {
                    self.handle_object_view_request(*request);
                    Ok(())
                }
                Command::Locomotion(request) => {
                    self.handle_locomotion_request(request);
                    Ok(())
                }
                Command::PhysicalResource(request) => {
                    self.handle_physical_resource(*request);
                    Ok(())
                }
                Command::MagicResource(request) => {
                    self.handle_magic_resource(request)
                        .expect("owner output capacity preflight");
                    Ok(())
                }
                Command::PlayerDeathService(command) => {
                    let outcome = self.apply_player_death_service(command);
                    self.player_deaths.outcomes.push_back(outcome);
                    Ok(())
                }
                Command::CorpseAccess(command) => {
                    self.apply_corpse_access_command(command);
                    Ok(())
                }
                Command::CorpseConsent(command) => {
                    self.apply_corpse_consent_command(command);
                    Ok(())
                }
                Command::PlayerDeath(command) => {
                    if let Err((error, command)) = self.apply_player_death_command(command) {
                        if matches!(
                            error,
                            crate::PlayerDeathError::Busy | crate::PlayerDeathError::Capacity
                        ) {
                            self.commands.push_front(Command::PlayerDeath(command));
                            break;
                        }
                        rejected.push(format!("player death: {error:?}"));
                    }
                    Ok(())
                }
                Command::Recall(command) => self
                    .apply_recall_command(command)
                    .map_err(|_| WorldError::InvalidMotion),
                Command::Generator(command) => {
                    let outcome = self.handle_generator_command(command);
                    self.generator_outcomes.push_back(outcome);
                    Ok(())
                }
                Command::Vendor(command) => {
                    let outcome = self.vendor_command(*command);
                    self.vendor_outcomes.push_back(outcome);
                    Ok(())
                }
                Command::PortalResolution(command) => {
                    let outcome = self.apply_portal_resolution(command);
                    self.portals.resolutions.push_back(outcome);
                    Ok(())
                }
                Command::Inventory(command) => self.handle_inventory_command(*command),
                Command::Crafting(command) => {
                    let result = self.apply_crafting_command(command);
                    self.crafting_outcomes.push_back(result);
                    Ok(())
                }
                Command::SkillDevice(command) => {
                    let result = self.apply_skill_device_command(command);
                    self.skill_device_outcomes.push_back(result);
                    Ok(())
                }
                Command::AttributeTransfer(command) => {
                    let result = self.apply_attribute_transfer_command(command);
                    self.attribute_transfer_outcomes.push_back(result);
                    Ok(())
                }
                Command::Ui { context, request } => {
                    let result = self.apply_ui(context, request);
                    self.ui_outcomes.push_back(ActionResult { context, result });
                    Ok(())
                }
                Command::TrainSkill { context, request } => {
                    let result = self
                        .propose_skill(context, crate::SkillIntent::Train(request))
                        .map(crate::SkillStage::Proposed);
                    self.skill_outcomes
                        .push_back(ActionResult { context, result });
                    Ok(())
                }
                Command::CommitSkill { ticket } => {
                    let result = self
                        .confirm_skill_committed(ticket)
                        .map(|_| crate::SkillStage::Committed(ticket));
                    self.skill_outcomes.push_back(ActionResult {
                        context: ticket.context,
                        result,
                    });
                    Ok(())
                }
                Command::RollbackSkill { ticket } => {
                    let result = self
                        .reject_skill(ticket)
                        .map(|_| crate::SkillStage::RolledBack(ticket));
                    self.skill_outcomes.push_back(ActionResult {
                        context: ticket.context,
                        result,
                    });
                    Ok(())
                }
                Command::UseDoor { context, request } => {
                    let result = self
                        .characters
                        .authorize(context, self.world.body(context.actor).is_ok())
                        .map_err(|e| match e {
                            ProgressionActionRejection::NotBound => DoorRejection::NotBound,
                            ProgressionActionRejection::OwnershipMismatch => {
                                DoorRejection::OwnershipMismatch
                            }
                            ProgressionActionRejection::StaleSequence => {
                                DoorRejection::StaleSequence
                            }
                            ProgressionActionRejection::MissingActor => DoorRejection::MissingActor,
                            _ => DoorRejection::InvalidState,
                        })
                        .and_then(|()| {
                            self.doors
                                .apply(context.actor, request.door, &self.world, self.tick)
                        });
                    self.door_outcomes
                        .push_back(ActionResult { context, result });
                    Ok(())
                }
                Command::Cast { context, request } => {
                    use bace_gameplay_api::CastRejection as E;
                    self.magic.refresh_item_targets(
                        &self.inventory,
                        match request {
                            bace_gameplay_api::CastRequest::Targeted { target, .. } => Some(target),
                            _ => None,
                        },
                    );
                    let result = if self.characters.reserved(context.actor)
                        || self.npcs.reserved(context.actor)
                        || self.inventory.reserved(context.actor)
                        || self.housing.reserved(context.actor)
                        || request != bace_gameplay_api::CastRequest::Cancel
                            && (self.recall_busy(context.actor)
                                || self.combat.active(context.actor)
                                || self.world.has_pending_action_motion(context.actor))
                    {
                        Err(E::Busy)
                    } else {
                        self.characters
                            .authorize(context, self.world.body(context.actor).is_ok())
                            .map_err(|e| match e {
                                ProgressionActionRejection::NotBound => E::NotBound,
                                ProgressionActionRejection::OwnershipMismatch => {
                                    E::OwnershipMismatch
                                }
                                ProgressionActionRejection::StaleSequence => E::StaleSequence,
                                ProgressionActionRejection::MissingActor => E::MissingActor,
                                _ => E::InvalidState,
                            })
                            .and_then(|()| {
                                self.magic.apply(
                                    context,
                                    request,
                                    &mut self.world,
                                    self.tick as f64 / 30.0,
                                    &self.combat,
                                    &self.fellowships,
                                    Some((&self.characters, self.tick)),
                                )
                            })
                    };
                    self.cast_outcomes
                        .push_back(ActionResult { context, result });
                    Ok(())
                }

                Command::Combat { context, request } => {
                    let authorization = self
                        .characters
                        .authorize(context, self.world.body(context.actor).is_ok());
                    let result = authorization
                        .map_err(|error| match error {
                            ProgressionActionRejection::NotBound => CombatRejection::NotBound,
                            ProgressionActionRejection::OwnershipMismatch => {
                                CombatRejection::OwnershipMismatch
                            }
                            ProgressionActionRejection::MissingActor => {
                                CombatRejection::MissingActor
                            }
                            ProgressionActionRejection::StaleSequence => {
                                CombatRejection::StaleSequence
                            }
                            ProgressionActionRejection::DurabilityPending => CombatRejection::Busy,
                            ProgressionActionRejection::Domain(_) => {
                                CombatRejection::InvalidRequest
                            }
                        })
                        .and_then(|()| {
                            if self.characters.reserved(context.actor) {
                                return Err(CombatRejection::Busy);
                            }
                            if matches!(
                                request,
                                CombatRequest::TargetedMelee { .. }
                                    | CombatRequest::TargetedMissile { .. }
                            ) && self
                                .player_deaths
                                .states
                                .get(&context.actor)
                                .is_some_and(|s| s.protection_elapsed.is_some())
                                && self.player_deaths.events.len() >= self.player_deaths.capacity
                            {
                                return Err(CombatRejection::Busy);
                            }
                            if matches!(
                                request,
                                CombatRequest::TargetedMelee { .. }
                                    | CombatRequest::TargetedMissile { .. }
                            ) && (self.recall_busy(context.actor)
                                || self.magic.busy(context.actor)
                                || self.world.has_pending_action_motion(context.actor)
                                    && !(self.combat.has_physical_driver(context.actor)
                                        && self
                                            .world
                                            .source_motion_token(context.actor)
                                            .is_some_and(|token| {
                                                token.domain == bace_motion::MotionDomain::Physical
                                            })))
                                || matches!(request, CombatRequest::ChangeMode(_))
                                    && (self.recall_busy(context.actor)
                                        || self.world.has_reserved_vitals(context.actor)
                                        || self.magic.mode_change_reserved(context.actor))
                            {
                                return Err(CombatRejection::Busy);
                            }
                            self.combat.apply_with_equipment(
                                &mut self.world,
                                context.actor,
                                request,
                                self.tick as f64 / 30.0,
                                &self.inventory,
                            )
                        });
                    if result.is_ok() {
                        match request {
                            CombatRequest::TargetedMelee { target, .. }
                            | CombatRequest::TargetedMissile { target, .. } => {
                                self.magic.set_damage_target(context.actor, Some(target));
                                self.dispel_lifestone_protection(context.actor)
                                    .expect("combat protection preflight");
                            }
                            CombatRequest::CancelAttack | CombatRequest::ChangeMode(1) => {
                                self.magic.set_damage_target(context.actor, None)
                            }
                            _ => {}
                        }
                    }
                    self.combat_outcomes
                        .push_back(ActionResult { context, result });
                    Ok(())
                }
                Command::RaiseProgression { context, request } => {
                    if let bace_gameplay_api::ProgressionTarget::Skill(skill) = request.target
                        && self
                            .characters
                            .can_take_complete(CharacterBinding {
                                actor: context.actor,
                                account: context.account,
                                session: context.session,
                            })
                            .is_ok()
                        && self.can_note_allegiance_skill_award(
                            context.actor,
                            skill,
                            request.amount,
                        ) == Err(bace_gameplay_api::social::SocialError::Capacity)
                    {
                        self.commands
                            .push_front(Command::RaiseProgression { context, request });
                        break;
                    }
                    let mut outcome = self.characters.apply(
                        context,
                        request,
                        self.world.body(context.actor).is_ok(),
                    );
                    let accepted = outcome.result.is_ok();
                    if accepted {
                        self.refresh_character_skills(context.actor)
                            .map_err(|_| SimulationError::SkillRefresh)?;
                        self.attach_rank_effect(
                            context.actor,
                            outcome.result.as_mut().expect("accepted progression"),
                        )
                        .map_err(|_| SimulationError::SkillRefresh)?;
                        if let bace_gameplay_api::ProgressionTarget::Skill(skill) = request.target {
                            self.note_allegiance_skill_award(context.actor, skill, request.amount)
                                .map_err(|_| SimulationError::SkillRefresh)?;
                        }
                    }
                    self.progression_outcomes.push_back(outcome);
                    Ok(())
                }
                Command::Movement {
                    actor,
                    epoch,
                    sequence,
                    intent,
                } => self.world.body_mut(actor).and_then(|body| {
                    body.submit_intent(epoch, sequence, intent)
                        .map_err(WorldError::from)
                }),
                Command::ServerTeleport {
                    actor,
                    cell,
                    position,
                } => self.world.teleport(actor, cell, position),
            };
            if let Err(error) = result {
                rejected.push(error.to_string());
            }
        }
        self.step_allegiance_checkpoint()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.step_region_residency()?;
        self.step_corpse_expiries();
        self.step_region_unloads();
        for (actor, error) in self.step_native_npcs() {
            rejected.push(format!("NPC {}: {error:?}", actor.0));
        }
        self.doors.step(&mut self.world, self.tick);
        self.stage_pve_shared_rewards()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.stage_npc_shared_rewards()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.population.maintain(&mut self.world, self.tick);
        self.step_generators()?;
        self.population
            .think(&mut self.world, &mut self.combat, self.tick, |actor| {
                self.magic.busy(actor)
                    || self.npcs.retiring(actor)
                    || self.npcs.admission_pending(actor)
            });
        self.service_monster_magic();
        self.population.start_melee_candidates(
            &mut self.world,
            &mut self.combat,
            &self.inventory,
            self.tick,
            |actor| {
                self.magic.busy(actor)
                    || self.npcs.retiring(actor)
                    || self.npcs.admission_pending(actor)
            },
        );
        self.world.tick()?;
        self.step_inventory_live();
        self.step_inventory_equipment_stances();
        self.step_crafting()?;
        self.doors.npc_contacts(&self.world, next_tick);
        self.service_physical_procs()?;
        self.combat.step_with_equipment(
            &mut self.world,
            next_tick as f64 / 30.0,
            &self.inventory,
            &self.characters,
        );
        self.service_physical_procs()?;
        self.step_recalls();
        self.service_portals();
        self.service_pets();
        self.service_magic_components();
        self.service_item_magic_procs()?;
        self.magic.refresh_item_targets(&self.inventory, None);
        self.magic.step(
            &mut self.world,
            next_tick as f64 / 30.0,
            &self.combat,
            &self.fellowships,
            Some((&self.characters, next_tick)),
        );
        self.service_item_magic_procs()?;
        for _ in 0..self.outcome_capacity {
            let Some(impact) = self.combat.peek_dirty() else {
                break;
            };
            if self
                .magic
                .apply_dirty_fighting(impact, &self.world, next_tick as f64 / 30.0)
                .is_err()
            {
                break;
            }
            self.combat.take_dirty();
        }
        self.refresh_changed_magic_skills()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.drain_cast_outcomes();
        self.step_generated_enchantments(next_tick)?;
        self.sync_registry_revisions()?;
        self.refresh_changed_physical_qualities()
            .map_err(|_| SimulationError::SkillRefresh)?;
        self.sync_recovery_revisions()?;
        self.sync_social_age()
            .map_err(|_| SimulationError::InvalidCommand)?;
        self.sync_player_world()?;
        self.refresh_dirty_locomotion()?;
        while self.combat_events.len() < self.outcome_capacity {
            let Some(event) = self
                .pending_magic_damage
                .take()
                .or_else(|| self.magic.take_combat_event())
                .or_else(|| self.npcs.take_damage_event())
            else {
                break;
            };
            if !self.ingest_combat_event(event, next_tick) {
                self.pending_magic_damage = Some(event);
                break;
            }
            self.combat_events.push_back(event);
        }
        while self.combat_events.len() < self.outcome_capacity {
            let Some(event) = self.combat.peek_event() else {
                break;
            };
            if !self.ingest_combat_event(event, next_tick) {
                break;
            }
            self.combat.take_event();
            self.combat_events.push_back(event);
        }
        self.tick = next_tick;
        self.drain_health_observations();
        Ok(rejected)
    }
}
