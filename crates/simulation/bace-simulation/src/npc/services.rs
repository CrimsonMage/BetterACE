//! Completion of named owner services. These methods verify the original exact
//! proposal; only the owning kernel adapter may call them after actual work.
use super::*;
pub(crate) trait NpcServiceView {
    fn reserved(&self, actor: EntityId) -> bool;
    fn experience_admission_supported(&self, actor: EntityId) -> Result<(), NpcFailure>;
    fn experience_recipient_supported(&self, actor: EntityId) -> Result<(), NpcFailure>;
    fn experience_modifier(&self, actor: EntityId) -> Result<f32, NpcFailure>;
    fn query(
        &self,
        context: NpcContext,
        query: &bace_gameplay_api::NpcQuery,
        raw_attribute: Option<u32>,
    ) -> Result<bace_gameplay_api::NpcQueryValue, NpcFailure>;
}
impl Npcs {
    pub(crate) fn seed_ticket_epoch(&mut self, epoch: u64) -> Result<(), NpcFailure> {
        if epoch == 0
            || epoch > u32::MAX as u64
            || !self.sources.is_empty()
            || self.next_ticket != 0
        {
            return Err(NpcFailure::Conflict);
        }
        self.next_ticket = epoch.checked_shl(32).ok_or(NpcFailure::Capacity)?;
        self.ticket_limit = self.next_ticket | u64::from(u32::MAX);
        Ok(())
    }

    pub(crate) fn freeze_idle(
        &mut self,
        source: EntityId,
        operation: u64,
        tick: u64,
    ) -> Result<NpcSourceCheckpoint, NpcFailure> {
        if operation == 0
            || self
                .pending
                .values()
                .any(|p| p.proposal.context.source == source)
            || self
                .handins
                .values()
                .any(|(h, _)| h.request.source == source)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        let state = self
            .sources
            .get_mut(&source)
            .ok_or(NpcFailure::MissingActor)?;
        if !state.recovery_ready
            || state.manager.busy()
            || state.manager.has_detached()
            || state.journal_hold.is_some_and(|(old, _)| old != operation)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        state
            .journal_hold
            .get_or_insert((operation, tick as f64 / 30.0 + state.clock_offset));
        self.source_checkpoint(source, tick)
    }
    pub(crate) fn release_idle(
        &mut self,
        source: EntityId,
        operation: u64,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        if self
            .pending
            .values()
            .any(|p| p.proposal.context.source == source)
        {
            return Err(NpcFailure::Conflict);
        }
        let state = self
            .sources
            .get_mut(&source)
            .ok_or(NpcFailure::MissingActor)?;
        let Some((held, now)) = state.journal_hold else {
            return Err(NpcFailure::Conflict);
        };
        if held != operation || state.manager.busy() || state.manager.has_detached() {
            return Err(NpcFailure::Conflict);
        }
        state.clock_offset = now - tick as f64 / 30.0;
        state.journal_hold = None;
        Ok(())
    }

    pub(crate) fn hold_journal(
        &mut self,
        expected: &NpcProposal,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        if self
            .pending
            .get(&expected.ticket)
            .is_none_or(|p| p.proposal != *expected)
        {
            return Err(NpcFailure::Conflict);
        }
        let source = self
            .sources
            .get_mut(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        if let Some((ticket, _)) = source.journal_hold {
            if ticket != expected.ticket {
                return Err(NpcFailure::DurabilityPending);
            }
            return Ok(());
        }
        source.journal_hold = Some((expected.ticket, tick as f64 / 30.0 + source.clock_offset));
        Ok(())
    }
    pub(crate) fn release_journal_hold(
        &mut self,
        expected: &NpcProposal,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        if self
            .pending
            .get(&expected.ticket)
            .is_none_or(|p| p.proposal != *expected)
        {
            return Err(NpcFailure::Conflict);
        }
        let source = self
            .sources
            .get_mut(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        if let Some((ticket, now)) = source.journal_hold {
            if ticket != expected.ticket {
                return Err(NpcFailure::Conflict);
            }
            source.clock_offset = now - tick as f64 / 30.0;
            source.journal_hold = None;
        }
        Ok(())
    }

    pub(crate) fn preview_owner_completion(
        &self,
        expected: &NpcProposal,
        tick: u64,
    ) -> Result<NpcSourceCheckpoint, NpcFailure> {
        let pending = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *expected || pending.adopted {
            return Err(NpcFailure::Conflict);
        }
        let now = self
            .source_checkpoint(expected.context.source, tick)?
            .logical_now;
        self.preview_committed(expected, pending.completion, now, tick)
    }

    pub(crate) fn retiring(&self, actor: EntityId) -> bool {
        self.pending.values().any(|p| {
            p.proposal.context.source == actor
                && matches!(
                    p.proposal.effect,
                    NpcEffect::Service(NpcOperation::DeleteSelf)
                )
        })
    }
    pub(crate) fn owns_generated_retirement(&self, operation: u64) -> bool {
        self.deletions.values().any(|t| {
            t.retirement
                .as_ref()
                .is_some_and(|r| r.inventory.operation == operation)
        })
    }
    pub(crate) fn damage_room(&self) -> bool {
        self.damage_events.len() < self.capacity
    }
    pub(crate) fn take_damage_event(&mut self) -> Option<crate::CombatEvent> {
        self.damage_events.pop_front()
    }
    pub(crate) fn snapshot_participant(&self, ticket: u64, actor: EntityId) -> bool {
        self.pending.get(&ticket).is_some_and(|p| {
            !p.adopted
                && (p.proposal.context.source == actor || p.proposal.context.target == Some(actor))
                && !matches!(p.proposal.effect, NpcEffect::QueuedExperience { .. })
        })
    }
    pub(crate) fn preview_service_admission(
        &self,
        expected: &NpcProposal,
        logical_now: f64,
        post_delay: f64,
        tick: u64,
    ) -> Result<NpcSourceCheckpoint, NpcFailure> {
        self.validate_service(expected)?;
        if self.service_detached(expected.ticket) {
            return Err(NpcFailure::Conflict);
        }
        let source = self
            .sources
            .get(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        let mut checkpoint = self.source_checkpoint(expected.context.source, tick)?;
        checkpoint.vm = source
            .manager
            .preview_detachment(expected.ticket, logical_now, post_delay)
            .map_err(|_| NpcFailure::Conflict)?;
        checkpoint.logical_now = logical_now;
        let pending = checkpoint
            .pending
            .iter_mut()
            .find(|p| p.proposal == *expected)
            .ok_or(NpcFailure::Conflict)?;
        pending.detached = true;
        pending.completion = NpcCompletion::Applied { post_delay };
        Ok(checkpoint)
    }
    pub(crate) fn admit_service(
        &mut self,
        expected: &NpcProposal,
        post_delay: f64,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        self.validate_service(expected)?;
        if self.service_detached(expected.ticket) {
            return Err(NpcFailure::Conflict);
        }
        self.release_journal_hold(expected, tick)?;
        let source = self
            .sources
            .get_mut(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        source
            .manager
            .detach_pending_without_execution(
                expected.ticket,
                tick as f64 / 30.0 + source.clock_offset,
                post_delay,
            )
            .map_err(|_| NpcFailure::Conflict)?;
        let pending = self
            .pending
            .get_mut(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        pending.detached = true;
        pending.completion = NpcCompletion::Applied { post_delay };
        self.proposals.retain(|p| p.ticket != expected.ticket);
        Ok(())
    }
    pub(crate) fn validate_service(&self, expected: &NpcProposal) -> Result<(), NpcFailure> {
        if self
            .sources
            .get(&expected.context.source)
            .and_then(|s| s.journal_hold)
            .is_some_and(|(ticket, _)| ticket != expected.ticket)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        let pending = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *expected
            || pending.adopted
            || !matches!(expected.effect, NpcEffect::Service(_))
        {
            return Err(NpcFailure::Conflict);
        }
        if !self
            .sources
            .get(&expected.context.source)
            .is_some_and(|s| s.recovery_ready)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        Ok(())
    }
    pub(crate) fn mark_service_adopted(
        &mut self,
        expected: &NpcProposal,
        completion: NpcCompletion,
    ) -> Result<(), NpcFailure> {
        self.validate_service(expected)?;
        let pending = self
            .pending
            .get_mut(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        pending.adopted = true;
        pending.completion = completion;
        Ok(())
    }
    pub(crate) fn service_detached(&self, ticket: u64) -> bool {
        self.pending.get(&ticket).is_some_and(|p| p.detached)
    }
    pub(crate) fn owns_inventory(&self, operation: u64) -> bool {
        self.inventory_services.contains_key(&operation) || self.handins.contains_key(&operation)
    }
    pub(crate) fn reserved_except(&self, actor: EntityId, ticket: u64) -> bool {
        self.sources
            .get(&actor)
            .and_then(|s| s.journal_hold)
            .is_some_and(|(held, _)| held != ticket)
            || self.pending.values().any(|p| {
                p.proposal.ticket != ticket
                    && !matches!(p.proposal.effect, NpcEffect::QueuedExperience { .. })
                    && (p.proposal.context.target == Some(actor)
                        || p.proposal.context.source == actor)
            })
    }
    pub(crate) fn pending_service(&self, ticket: u64) -> Option<&NpcProposal> {
        self.pending
            .get(&ticket)
            .map(|p| &p.proposal)
            .filter(|p| matches!(p.effect, NpcEffect::Service(_)))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit borrowed subsystem owners; no duplicated aggregate state"
    )]
    pub(crate) fn complete_service(
        &mut self,
        expected: &NpcProposal,
        completion: NpcCompletion,
        world: &mut World,
        characters: &mut Characters,
        fellowships: &mut crate::fellowships::Fellowships,
        tick: u64,
        services: &dyn NpcServiceView,
    ) -> Result<(), NpcFailure> {
        let pending = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *expected
            || !matches!(expected.effect, NpcEffect::Service(_))
            || matches!(completion,NpcCompletion::Branch{category}if category>38)
            || matches!(
                completion,
                NpcCompletion::Pending { .. } | NpcCompletion::Detached { .. }
            )
        {
            return Err(NpcFailure::Conflict);
        }
        if pending.detached {
            self.sources
                .get_mut(&expected.context.source)
                .ok_or(NpcFailure::MissingActor)?
                .manager
                .complete_detached(expected.ticket)
                .map_err(|_| NpcFailure::Conflict)?;
            self.pending.remove(&expected.ticket);
            self.proposals.retain(|p| p.ticket != expected.ticket);
            Ok(())
        } else {
            self.complete(
                expected.ticket,
                completion,
                world,
                characters,
                fellowships,
                tick,
                services,
            )
        }
    }
    pub(crate) fn checkpoint(
        &self,
        source: EntityId,
    ) -> Result<bace_emotes::NativeCheckpoint, NpcFailure> {
        Ok(self
            .sources
            .get(&source)
            .ok_or(NpcFailure::MissingActor)?
            .manager
            .checkpoint())
    }
}
impl Npcs {
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit borrowed subsystem owners; no duplicated aggregate state"
    )]
    pub(crate) fn detach_service(
        &mut self,
        expected: &NpcProposal,
        post_delay: f64,
        world: &mut World,
        characters: &mut Characters,
        fellowships: &mut crate::fellowships::Fellowships,
        tick: u64,
        services: &dyn NpcServiceView,
    ) -> Result<(), NpcFailure> {
        let pending = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *expected
            || pending.detached
            || !matches!(expected.effect, NpcEffect::Service(_))
        {
            return Err(NpcFailure::Conflict);
        }
        let mut source = self
            .sources
            .remove(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        let now = tick as f64 / 30.0 + source.clock_offset;
        let result = (|| {
            let random = source.random.as_mut().ok_or(NpcFailure::MissingContent)?;
            let mut host = host::Host {
                state: self,
                world,
                characters,
                fellowships,
                random,
                tick,
                services,
            };
            source
                .manager
                .detach_pending(expected.ticket, post_delay, now, &mut host)
                .map_err(|_| NpcFailure::Conflict)
        })();
        self.sources.insert(expected.context.source, source);
        result?;
        self.pending
            .get_mut(&expected.ticket)
            .expect("retained")
            .detached = true;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcPendingCheckpoint {
    pub proposal: NpcProposal,
    pub completion: NpcCompletion,
    pub adopted: bool,
    pub detached: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcInvocationCheckpoint {
    pub operation: u64,
    pub event_id: [u8; 16],
    pub key_version: u32,
    pub random_position: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcSourceCheckpoint {
    pub inventory: Option<NpcSourceInventorySnapshot>,
    pub location: Option<NpcSourceLocation>,
    pub properties: Option<bace_entity::EntityProperties>,
    pub source_quests: Option<(u64, Vec<(String, bace_quests::QuestProgress)>)>,
    pub archive: Option<NpcSourceArchive>,
    pub source: EntityId,
    pub active_operation: u64,
    pub invocations: Vec<NpcInvocationCheckpoint>,
    pub logical_now: f64,
    pub event_id: [u8; 16],
    pub key_version: u32,
    pub random_position: u64,
    pub vm: bace_emotes::NativeCheckpoint,
    pub pending: Vec<NpcPendingCheckpoint>,
}
impl Npcs {
    pub(crate) fn source_checkpoint(
        &self,
        source: EntityId,
        tick: u64,
    ) -> Result<NpcSourceCheckpoint, NpcFailure> {
        let state = self.sources.get(&source).ok_or(NpcFailure::MissingActor)?;
        let mut invocations = state.invocations.clone();
        invocations.insert(
            state.active_operation,
            NpcInvocationCheckpoint {
                operation: state.active_operation,
                event_id: state.event_id,
                key_version: state.key_version,
                random_position: state.random.as_ref().map_or(0, RandomStream::position),
            },
        );
        Ok(NpcSourceCheckpoint {
            inventory: self.source_inventory.get(&source).cloned(),
            location: None,
            properties: None,
            source_quests: self.quests.get(&source).map(|q| {
                (
                    q.revision(),
                    q.iter()
                        .map(|(name, value)| (name.to_owned(), value))
                        .collect(),
                )
            }),
            archive: self.archives.get(&source).cloned(),
            active_operation: state.active_operation,
            invocations: invocations.into_values().collect(),
            source,
            logical_now: state
                .journal_hold
                .map(|(_, now)| now)
                .or(state.held_logical_now)
                .unwrap_or(tick as f64 / 30.0 + state.clock_offset),
            event_id: state.event_id,
            key_version: state.key_version,
            random_position: state.random.as_ref().map_or(0, RandomStream::position),
            vm: state.manager.checkpoint(),
            pending: self
                .pending
                .values()
                .filter(|p| p.proposal.context.source == source)
                .map(|p| NpcPendingCheckpoint {
                    proposal: p.proposal.clone(),
                    completion: p.completion,
                    adopted: p.adopted,
                    detached: p.detached,
                })
                .collect(),
        })
    }
    pub(crate) fn restore_source(
        &mut self,
        snapshot: NpcSourceCheckpoint,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        if snapshot
            .pending
            .iter()
            .map(|p| p.proposal.ticket)
            .chain(snapshot.vm.pending.iter().map(|p| p.ticket))
            .chain(snapshot.vm.detached.iter().copied())
            .any(|ticket| ticket > self.ticket_limit)
        {
            return Err(NpcFailure::InvalidInput);
        }
        let restored_quests = snapshot
            .source_quests
            .as_ref()
            .map(|(revision, rows)| {
                QuestRegistry::restore_snapshot(*revision, rows.clone())
                    .map_err(|_| NpcFailure::InvalidInput)
            })
            .transpose()?;
        if restored_quests.is_some()
            && !self.quests.contains_key(&snapshot.source)
            && self.quests.len() >= 4096
        {
            return Err(NpcFailure::Capacity);
        }
        if let Some(archive) = &snapshot.archive {
            archive.validate()?;
            if archive.source != snapshot.source {
                return Err(NpcFailure::InvalidInput);
            }
            if self.archives.contains_key(&snapshot.source) {
                return Err(NpcFailure::Conflict);
            }
            if self
                .archives
                .values()
                .try_fold(
                    archive.retained_bytes().ok_or(NpcFailure::Capacity)?,
                    |n, a| n.checked_add(a.retained_bytes()?),
                )
                .is_none_or(|n| n > 64 * 1024 * 1024)
            {
                return Err(NpcFailure::Capacity);
            }
        }
        let source = self
            .sources
            .get(&snapshot.source)
            .ok_or(NpcFailure::MissingActor)?;
        if source.manager.busy()
            || source.manager.has_detached()
            || self
                .pending
                .values()
                .any(|p| p.proposal.context.source == snapshot.source)
        {
            return Err(NpcFailure::Conflict);
        }
        if !snapshot.logical_now.is_finite()
            || snapshot.logical_now < 0.0
            || snapshot.random_position > 65536
            || snapshot.pending.len() + self.pending.len() > self.capacity
            || snapshot.pending.len() + self.proposals.len() > self.capacity
        {
            return Err(NpcFailure::Capacity);
        }
        if snapshot.invocations.len() > 76
            || !snapshot.invocations.iter().any(|i| {
                i.operation == snapshot.active_operation
                    && i.event_id == snapshot.event_id
                    && i.key_version == snapshot.key_version
                    && i.random_position == snapshot.random_position
            })
            || snapshot.invocations.iter().enumerate().any(|(index, v)| {
                v.event_id == [0; 16]
                    || v.key_version == 0
                    || v.random_position > 65536
                    || snapshot.invocations[..index]
                        .iter()
                        .any(|o| o.operation == v.operation)
            })
            || snapshot.pending.iter().any(|p| {
                !snapshot
                    .invocations
                    .iter()
                    .any(|i| i.operation == p.proposal.context.operation)
            })
        {
            return Err(NpcFailure::InvalidInput);
        }
        let root = self.root.as_ref().ok_or(NpcFailure::MissingContent)?;
        if root.key_version() != snapshot.key_version {
            return Err(NpcFailure::MissingContent);
        }
        for (index, p) in snapshot.pending.iter().enumerate() {
            if let NpcEffect::QueuedExperience { amount, phase, .. } = p.proposal.effect
                && (amount > i64::MAX as u64
                    || p.adopted
                    || p.detached != (phase == NpcQueuedExperiencePhase::Ready))
            {
                return Err(NpcFailure::InvalidInput);
            }
            if p.proposal.context.source != snapshot.source
                || self.pending.contains_key(&p.proposal.ticket)
                || snapshot.pending[..index]
                    .iter()
                    .any(|old| old.proposal.ticket == p.proposal.ticket)
                || if p.detached {
                    !snapshot.vm.detached.contains(&p.proposal.ticket)
                } else {
                    !snapshot
                        .vm
                        .pending
                        .iter()
                        .any(|v| v.ticket == p.proposal.ticket)
                }
            {
                return Err(NpcFailure::Conflict);
            }
        }
        if snapshot.vm.pending.len() + snapshot.vm.detached.len() != snapshot.pending.len() {
            return Err(NpcFailure::Conflict);
        }
        if snapshot
            .vm
            .work
            .iter()
            .any(|w| w.context.source != snapshot.source)
            || snapshot
                .vm
                .pending
                .iter()
                .any(|w| w.row.context.source != snapshot.source)
        {
            return Err(NpcFailure::Conflict);
        }
        let vm = NativeEmoteManager::restore(source.manager.program().clone(), snapshot.vm)
            .map_err(|_| NpcFailure::InvalidInput)?;
        let mut random = root
            .event_stream(snapshot.event_id, Domain::Npc)
            .map_err(|_| NpcFailure::InvalidInput)?;
        random.seek(snapshot.random_position);
        if !snapshot.pending.is_empty() || vm.busy() || vm.has_detached() {
            self.durable_dirty.insert(snapshot.source);
        }
        let source = self
            .sources
            .get_mut(&snapshot.source)
            .expect("checkedsource");
        source.active_operation = snapshot.active_operation;
        source.invocations = snapshot
            .invocations
            .into_iter()
            .map(|i| (i.operation, i))
            .collect();
        source.recovery_ready = false;
        source.held_logical_now = Some(snapshot.logical_now);
        source.manager = vm;
        source.random = Some(random);
        source.event_id = snapshot.event_id;
        source.key_version = snapshot.key_version;
        source.clock_offset = snapshot.logical_now - tick as f64 / 30.0;
        if let Some(quests) = restored_quests {
            self.quests.insert(snapshot.source, quests);
        }
        if let Some(archive) = snapshot.archive {
            self.archives.insert(snapshot.source, archive);
        }
        for p in snapshot.pending {
            self.next_ticket = self.next_ticket.max(p.proposal.ticket);
            if !p.adopted
                && !matches!(
                    p.proposal.effect,
                    NpcEffect::QueuedExperience {
                        phase: NpcQueuedExperiencePhase::Ready,
                        ..
                    }
                )
            {
                self.proposals.push_back(p.proposal.clone());
            }
            self.pending.insert(
                p.proposal.ticket,
                Pending {
                    proposal: p.proposal,
                    completion: p.completion,
                    adopted: p.adopted,
                    detached: p.detached,
                },
            );
        }
        Ok(())
    }
}

impl Npcs {
    pub(crate) fn register_contract_definitions(
        &mut self,
        ids: Vec<u32>,
    ) -> Result<(), NpcFailure> {
        self.register_contract_catalog(ids.into_iter().map(|id| (id, String::new())).collect())
    }
}

impl Npcs {
    pub(crate) fn register_experience(
        &mut self,
        table: Arc<bace_character::CharacterLevelTable>,
        global: f64,
        quest: f64,
    ) -> Result<(), NpcFailure> {
        if self.level_table.is_some()
            || ![global, quest]
                .into_iter()
                .all(|v| v.is_finite() && v >= 0.0)
        {
            return Err(NpcFailure::InvalidInput);
        }
        self.level_table = Some(table);
        self.xp_rates = Some((global, quest));
        Ok(())
    }
}

impl Npcs {
    pub(crate) fn register_contract_catalog(
        &mut self,
        entries: Vec<(u32, String)>,
    ) -> Result<(), NpcFailure> {
        if self.contract_catalog_loaded
            || entries.len() > 100000
            || entries
                .iter()
                .any(|(id, name)| *id == 0 || name.len() > 4096)
        {
            return Err(NpcFailure::InvalidInput);
        }
        let count = entries.len();
        let prepared: BTreeMap<_, _> = entries.into_iter().collect();
        if prepared.len() != count {
            return Err(NpcFailure::InvalidInput);
        }
        self.contract_definitions = prepared;
        self.contract_catalog_loaded = true;
        Ok(())
    }
}

impl Npcs {
    pub(crate) fn preview_committed(
        &self,
        expected: &NpcProposal,
        completion: NpcCompletion,
        logical_now: f64,
        tick: u64,
    ) -> Result<NpcSourceCheckpoint, NpcFailure> {
        let current = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if matches!(expected.effect, NpcEffect::QueuedExperience { .. }) {
            return Err(NpcFailure::Unsupported);
        }
        if current.proposal != *expected
            || !logical_now.is_finite()
            || logical_now < 0.0
            || matches!(completion,NpcCompletion::Branch{category}if category>38)
            || matches!(
                completion,
                NpcCompletion::Pending { .. } | NpcCompletion::Detached { .. }
            )
            || matches!(completion,NpcCompletion::Applied{post_delay}if !post_delay.is_finite()||post_delay<0.0)
        {
            return Err(NpcFailure::InvalidInput);
        }
        let mut checkpoint = self.source_checkpoint(expected.context.source, tick)?;
        checkpoint.logical_now = logical_now;
        if let (Some(archive), NpcEffect::Property { actor, change, .. }) =
            (&mut checkpoint.archive, &expected.effect)
            && archive.source == *actor
        {
            archive
                .properties
                .adopt(change.clone())
                .map_err(|_| NpcFailure::Conflict)?;
        }
        if let NpcEffect::Quest { actor, change, .. } = &expected.effect
            && *actor == checkpoint.source
        {
            let (revision, rows) = checkpoint
                .source_quests
                .as_ref()
                .ok_or(NpcFailure::MissingContent)?;
            let mut quests = QuestRegistry::restore_snapshot(*revision, rows.clone())
                .map_err(|_| NpcFailure::InvalidInput)?;
            quests
                .adopt(change.clone())
                .map_err(|_| NpcFailure::Conflict)?;
            checkpoint.source_quests = Some((
                quests.revision(),
                quests
                    .iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ));
        }
        let pending = checkpoint
            .pending
            .iter_mut()
            .find(|p| p.proposal.ticket == expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        pending.adopted = true;
        pending.completion = completion;
        Ok(checkpoint)
    }
}

impl Npcs {
    /// Trusted runtime acknowledgment: every durable participant referenced by this
    /// source checkpoint has been restored before query/action execution resumes.
    pub(crate) fn acknowledge_recovery_ready(
        &mut self,
        source: EntityId,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        let state = self
            .sources
            .get_mut(&source)
            .ok_or(NpcFailure::MissingActor)?;
        if state.admission.is_some() && !state.admission_bound {
            return Err(NpcFailure::Conflict);
        }
        if let Some(now) = state.held_logical_now.take() {
            state.clock_offset = now - tick as f64 / 30.0;
        }
        state.recovery_ready = true;
        Ok(())
    }
}
