//! Single-owner kernel npc operations.
mod admission;
mod casting;
mod commands;
mod death;
mod deletion;
mod doors;
mod generators;
mod handin;
mod inventory;
mod motion;
mod movement;
mod skill_reset;
mod source_inventory;
mod source_state;
mod spellbook;
mod teleport;
mod training_credits;
use super::npc_services::NativeReadServices;
use super::*;
impl Kernel {
    pub fn seed_npc_ticket_epoch(
        &mut self,
        epoch: u64,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.seed_ticket_epoch(epoch)
    }
    pub fn use_native_npc(
        &mut self,
        context: bace_gameplay_api::ActionContext,
        source: EntityId,
        event: [u8; 16],
        operation: u64,
    ) -> Result<bool, bace_gameplay_api::NpcFailure> {
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| bace_gameplay_api::NpcFailure::InvalidInput)?;
        if self
            .world
            .combatant(context.actor)
            .is_none_or(|c| c.health() == 0)
            || self.world.is_in_portal_transit(context.actor)
        {
            return Err(bace_gameplay_api::NpcFailure::InvalidInput);
        }
        self.start_npc_emote(
            source,
            Some(context.actor),
            bace_emotes::NativeTrigger {
                category: 7,
                ..Default::default()
            },
            event,
            operation,
            true,
        )
        .map_err(|error| match error {
            bace_emotes::NativeError::Owner(error) => error,
            bace_emotes::NativeError::Busy => bace_gameplay_api::NpcFailure::DurabilityPending,
            _ => bace_gameplay_api::NpcFailure::InvalidInput,
        })
    }

    pub fn preview_npc_owner_completion(
        &mut self,
        expected: &crate::NpcProposal,
    ) -> Result<crate::NpcSourceCheckpoint, bace_gameplay_api::NpcFailure> {
        let preview = self.npcs.preview_owner_completion(expected, self.tick)?;
        let preview = self.capture_npc_source_state(preview)?;
        self.npcs.hold_journal(expected, self.tick)?;
        Ok(preview)
    }

    pub fn register_native_npc_with_properties(
        &mut self,
        actor: EntityId,
        program: std::sync::Arc<bace_emotes::NativeProgram>,
        use_radius: f32,
        properties: bace_entity::EntityProperties,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if properties
            .retained_bytes()
            .is_none_or(|n| n > 2 * 1024 * 1024)
        {
            return Err(bace_gameplay_api::NpcFailure::InvalidInput);
        }
        self.npcs
            .register(actor, program, use_radius, &self.world)?;
        if self.world.properties(actor).is_none() {
            self.world
                .register_properties(actor, properties)
                .expect("single-owner source/property registration preflight");
        }
        Ok(())
    }
    pub fn register_archived_npc(
        &mut self,
        snapshot: crate::NpcSourceCheckpoint,
        program: std::sync::Arc<bace_emotes::NativeProgram>,
        use_radius: f32,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self.world.contains_identity(snapshot.source)
            || self.characters.get(snapshot.source).is_some()
        {
            return Err(bace_gameplay_api::NpcFailure::Conflict);
        }
        self.npcs
            .register_archived(snapshot, program, use_radius, self.tick)
    }
    pub fn release_npc_archive(
        &mut self,
        source: EntityId,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.release_archive(source)
    }
    pub fn apply_npc_signal(
        &mut self,
        expected: &crate::NpcProposal,
    ) -> Result<usize, bace_gameplay_api::NpcFailure> {
        let count = self
            .npcs
            .signal(expected, &self.world, self.tick, &self.inventory)?;
        self.npcs.mark_service_adopted(
            expected,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
        )?;
        self.confirm_npc_committed(expected)?;
        Ok(count)
    }
    pub fn validate_npc_snapshot(&self, ticket: u64, actor: EntityId, revision: u64) -> bool {
        self.npcs.snapshot_participant(ticket, actor)
            && self
                .characters
                .get(actor)
                .is_some_and(|c| c.revision() == revision)
    }
    pub fn preview_npc_service_admission(
        &self,
        expected: &crate::NpcProposal,
        logical_now: f64,
        post_delay: f64,
    ) -> Result<crate::NpcSourceCheckpoint, bace_gameplay_api::NpcFailure> {
        self.npcs
            .preview_service_admission(expected, logical_now, post_delay, self.tick)
            .and_then(|snapshot| self.capture_npc_source_state(snapshot))
    }
    /// Adopt only the exact workflow-only durable admission receipt. Owner work
    /// remains pending while the source continuation runs with its authored delay.
    pub fn admit_npc_service(
        &mut self,
        expected: &crate::NpcProposal,
        post_delay: f64,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.admit_service(expected, post_delay, self.tick)
    }
    pub fn configure_npc_services(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
        epoch: u32,
        quests: Vec<(String, bace_quests::QuestDefinition)>,
        events: bace_world_events::Events,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.configure(root, epoch, quests, events)
    }
    pub fn register_native_npc(
        &mut self,
        actor: EntityId,
        program: std::sync::Arc<bace_emotes::NativeProgram>,
        use_radius: f32,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.register(actor, program, use_radius, &self.world)
    }
    pub fn start_npc_emote(
        &mut self,
        source: EntityId,
        target: Option<EntityId>,
        trigger: bace_emotes::NativeTrigger,
        event: [u8; 16],
        operation: u64,
        check_range: bool,
    ) -> Result<bool, bace_emotes::NativeError> {
        self.npcs.start(
            crate::npc::NativeInvocation {
                actor: source,
                target,
                trigger,
                event,
                operation,
                check_range,
            },
            &mut self.world,
            &mut self.characters,
            &mut self.fellowships,
            self.tick,
            &NativeReadServices {
                inventory: &self.inventory,
                combat: &self.combat,
                magic: &self.magic,
                portals: &self.portals,
                housing: &self.housing,
            },
        )
    }
    pub fn take_npc_proposal(&mut self) -> Option<crate::NpcProposal> {
        self.npcs.take_proposal()
    }
    pub fn take_npc_notification(&mut self) -> Option<crate::NpcNotification> {
        self.npcs.take_notification()
    }
    pub fn confirm_npc_committed(
        &mut self,
        receipt: &crate::NpcProposal,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self
            .allegiances
            .pending
            .as_ref()
            .and_then(|t| t.npc.as_ref())
            .is_some_and(|p| p.ticket == receipt.ticket)
        {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        let sanctuary =
            if let crate::NpcEffect::CharacterService { actor, change } = &receipt.effect {
                (change.before.sanctuary != change.after.sanctuary)
                    .then_some((*actor, change.after.sanctuary))
            } else {
                None
            };
        if sanctuary.is_some_and(|(actor, _)| self.portal_reserved(actor)) {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        if let Some((actor, position)) = sanctuary
            && let Some(links) = self.portal_links(actor)
        {
            let mut preview = links.clone();
            preview
                .synchronize_sanctuary(position.map(|p| bace_interactions::PortalPosition {
                    cell: p.cell.map_or(0, |c| c.0),
                    origin: [p.position.x, p.position.y, p.position.z],
                    rotation: p.rotation,
                }))
                .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)?;
        }
        let scalar_actor = if let crate::NpcEffect::Property {
            actor,
            aggregate: None,
            change,
        } = &receipt.effect
        {
            if let Some(properties) = self.world.properties(*actor) {
                let mut candidate = properties.clone();
                candidate
                    .adopt(change.clone())
                    .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)?;
                self.validate_npc_scalar_properties(*actor, &candidate)
                    .map_err(|_| bace_gameplay_api::NpcFailure::InvalidInput)?;
                Some(*actor)
            } else {
                None
            }
        } else {
            None
        };
        self.npcs.confirm(
            receipt,
            &mut self.world,
            &mut self.characters,
            &mut self.fellowships,
            self.tick,
            &NativeReadServices {
                inventory: &self.inventory,
                combat: &self.combat,
                magic: &self.magic,
                portals: &self.portals,
                housing: &self.housing,
            },
        )?;
        if let Some(actor) = scalar_actor {
            self.refresh_npc_scalar_source(actor)
                .map_err(|_| bace_gameplay_api::NpcFailure::InvalidInput)?;
        }
        if let Some((actor, position)) = sanctuary {
            self.synchronize_portal_sanctuary(
                actor,
                position.map(|p| bace_interactions::PortalPosition {
                    cell: p.cell.expect("validated sanctuary").0,
                    origin: [p.position.x, p.position.y, p.position.z],
                    rotation: p.rotation,
                }),
            )
            .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)?;
        }
        Ok(())
    }
    pub fn register_fellowship(
        &mut self,
        fellow: bace_fellowship::Fellowship,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if fellow
            .members()
            .iter()
            .any(|actor| self.characters.get(*actor).is_none())
        {
            return Err(bace_gameplay_api::NpcFailure::MissingActor);
        }
        self.fellowships
            .register(fellow)
            .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)
    }
    pub fn register_quest_registry(
        &mut self,
        actor: EntityId,
        quests: bace_quests::QuestRegistry,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self.world.body(actor).is_err() {
            return Err(bace_gameplay_api::NpcFailure::MissingActor);
        }
        self.npcs.register_quests(actor, quests)
    }
    pub fn npc_quest(&self, actor: EntityId, name: &str) -> Option<bace_quests::QuestProgress> {
        self.npcs.quest(actor, name)
    }
    /// Discard only quiescent source registrations after every physical actor has left.
    /// Durable continuations, receipts and outward notifications remain recovery work.
    pub fn discard_idle_npc_sources(&mut self) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self.world.states().next().is_some() || !self.npc_service_outcomes.is_empty() {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        self.npcs.discard_idle_sources()
    }
    pub fn has_npc_state(&self) -> bool {
        self.npcs.has_state()
    }
    pub fn spawn_npc(&mut self, actor: EntityId, blueprint: NpcBlueprint) -> Result<(), PveError> {
        if self.magic.reserves_identity(actor)
            || self.inventory.reserved(actor)
            || self.inventory.item(actor).is_some()
        {
            return Err(PveError::Duplicate);
        }
        self.population
            .spawn(actor, blueprint, &mut self.world, self.tick)
    }
    pub fn set_npc_leash(
        &mut self,
        actor: EntityId,
        leash: bace_ai::MonsterLeash,
    ) -> Result<(), PveError> {
        self.population.set_leash(actor, leash)
    }
}

impl Kernel {
    pub fn npc_pending_service(&self, ticket: u64) -> Option<&crate::NpcProposal> {
        self.npcs.pending_service(ticket)
    }
    pub fn complete_npc_service(
        &mut self,
        expected: &crate::NpcProposal,
        completion: bace_gameplay_api::NpcCompletion,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self.npcs.deletions.contains_key(&expected.ticket) {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        if self.npcs.skill_resets.contains_key(&expected.ticket) {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        if self
            .npcs
            .portal_services
            .values()
            .any(|p| p.npc.ticket == expected.ticket)
        {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        if self
            .npcs
            .inventory_services
            .values()
            .any(|t| t.npc.ticket == expected.ticket)
            || self.npcs.cast_services.contains_key(&expected.ticket)
            || self.npcs.motion_services.contains_key(&expected.ticket)
            || self.npcs.moves.contains_key(&expected.ticket)
            || self.npcs.spellbook_services.contains_key(&expected.ticket)
            || self
                .npcs
                .training_credit_services
                .contains_key(&expected.ticket)
        {
            return Err(bace_gameplay_api::NpcFailure::DurabilityPending);
        }
        self.npcs.complete_service(
            expected,
            completion,
            &mut self.world,
            &mut self.characters,
            &mut self.fellowships,
            self.tick,
            &NativeReadServices {
                inventory: &self.inventory,
                combat: &self.combat,
                magic: &self.magic,
                portals: &self.portals,
                housing: &self.housing,
            },
        )
    }
    pub fn npc_checkpoint(
        &self,
        source: EntityId,
    ) -> Result<bace_emotes::NativeCheckpoint, bace_gameplay_api::NpcFailure> {
        self.npcs.checkpoint(source)
    }
}

impl Kernel {
    pub fn detach_npc_service(
        &mut self,
        expected: &crate::NpcProposal,
        post_delay: f64,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.detach_service(
            expected,
            post_delay,
            &mut self.world,
            &mut self.characters,
            &mut self.fellowships,
            self.tick,
            &NativeReadServices {
                inventory: &self.inventory,
                combat: &self.combat,
                magic: &self.magic,
                portals: &self.portals,
                housing: &self.housing,
            },
        )
    }
}

impl Kernel {
    pub fn checkpoint_npc_source(
        &self,
        source: EntityId,
    ) -> Result<crate::npc::NpcSourceCheckpoint, bace_gameplay_api::NpcFailure> {
        self.capture_npc_source_state(self.npcs.source_checkpoint(source, self.tick)?)
    }
    pub fn restore_npc_source(
        &mut self,
        snapshot: crate::npc::NpcSourceCheckpoint,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if snapshot.archive.is_some() && self.world.contains_identity(snapshot.source) {
            return Err(bace_gameplay_api::NpcFailure::Conflict);
        }
        self.restore_npc_source_state(snapshot)
    }
}

impl Kernel {
    pub fn register_npc_character_services(
        &mut self,
        actor: EntityId,
        state: bace_character::CharacterServiceState,
        contracts: bace_quests::ContractRegistry,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.characters
            .register_native_services(actor, state, contracts)
            .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)
    }
    pub fn npc_character_services(
        &self,
        actor: EntityId,
    ) -> Option<&bace_character::CharacterServiceState> {
        self.characters.native_services(actor)
    }
    pub fn npc_contracts(&self, actor: EntityId) -> Option<&bace_quests::ContractRegistry> {
        self.characters.contracts(actor)
    }
    pub fn register_npc_contract_definitions(
        &mut self,
        ids: Vec<u32>,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.register_contract_definitions(ids)
    }
}

impl Kernel {
    pub fn register_npc_experience_table(
        &mut self,
        table: std::sync::Arc<bace_character::CharacterLevelTable>,
        global: f64,
        quest: f64,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.register_experience(table, global, quest)
    }
}
impl Kernel {
    pub fn fellowship_members(&self, actor: EntityId) -> Option<&[EntityId]> {
        self.fellowships.roster(actor)
    }
}

impl Kernel {
    pub fn register_npc_contract_catalog(
        &mut self,
        entries: Vec<(u32, String)>,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.register_contract_catalog(entries)
    }
}

impl Kernel {
    /// Refresh a derived casting permission only from an already accepted canonical
    /// spellbook. This cannot teach a spell or fabricate a persistence receipt.
    pub fn synchronize_npc_known_spell(
        &mut self,
        actor: EntityId,
        spell: u32,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        if self.characters.knows_spell(actor, spell) != Some(true) {
            return Err(bace_gameplay_api::NpcFailure::Conflict);
        }
        self.magic.adopt_known_spell(actor, spell);
        Ok(())
    }
    pub fn preview_npc_committed_checkpoint(
        &self,
        expected: &crate::NpcProposal,
        completion: bace_gameplay_api::NpcCompletion,
        logical_now: f64,
    ) -> Result<crate::NpcSourceCheckpoint, bace_gameplay_api::NpcFailure> {
        self.npcs
            .preview_committed(expected, completion, logical_now, self.tick)
            .and_then(|snapshot| self.capture_npc_source_state(snapshot))
    }
}

impl Kernel {
    /// The recovery adapter must load all checkpoint participant owners first.
    /// This is deliberately not a client command or an implicit existence default.
    pub fn acknowledge_npc_recovery_ready(
        &mut self,
        source: EntityId,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        let checkpoint = self.npcs.source_checkpoint(source, self.tick)?;
        for target in checkpoint
            .vm
            .work
            .iter()
            .chain(checkpoint.vm.pending.iter().map(|p| &p.row))
            .filter_map(|row| row.context.target)
            .chain(
                checkpoint
                    .pending
                    .iter()
                    .filter_map(|p| p.proposal.context.target),
            )
        {
            if target != source && self.world.actor_state(target).is_err() {
                return Err(bace_gameplay_api::NpcFailure::MissingActor);
            }
        }
        let hold = self.npcs.admission_hold(source);
        if let Some(hold) = hold
            && self.world.npc_admission_hold(source) != Some(hold)
        {
            return Err(bace_gameplay_api::NpcFailure::Conflict);
        }
        self.npcs.acknowledge_recovery_ready(source, self.tick)?;
        if let Some(hold) = hold {
            self.world
                .finish_npc_admission(source, hold)
                .map_err(|_| bace_gameplay_api::NpcFailure::Conflict)?;
            self.npcs.set_admission_hold(source, None);
        }
        Ok(())
    }
}
impl Kernel {
    pub fn preview_npc_queued_experience_admission(
        &self,
        expected: &crate::NpcProposal,
        logical_now: f64,
    ) -> Result<crate::NpcSourceCheckpoint, bace_gameplay_api::NpcFailure> {
        self.npcs
            .preview_experience_admission(expected, logical_now, self.tick)
            .and_then(|snapshot| self.capture_npc_source_state(snapshot))
    }
    /// Call only after the workflow-only admission checkpoint is durable.
    pub fn admit_npc_queued_experience(
        &mut self,
        expected: &crate::NpcProposal,
    ) -> Result<(), bace_gameplay_api::NpcFailure> {
        self.npcs.admit_queued_experience(expected, self.tick)
    }
}
