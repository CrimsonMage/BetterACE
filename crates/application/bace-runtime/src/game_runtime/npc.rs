//! Production NPC service adapter. Cold definitions, canonical source heads,
//! exact save stages, and simulation receipts have separately retained owners.
mod casting;
mod effects;
mod giving;
mod handin;
mod inventory_output;
mod motions;
mod publications;
mod regions;
mod shutdown;
mod source_inventory;
mod stages;
mod terminal;
use super::*;
use crate::{
    npc_persistence::NpcCheckpointBinding,
    npc_recovery::{NpcRecoveryRequest, NpcRecoveryWorker, PreparedNpcRecovery},
    npc_service::{NpcCoordinator, NpcCoordinatorEvent},
    npc_sources::PreparedNpcRegistration,
};
use bace_gameplay_api::{NpcOperation, NpcRewardKind};
use bace_simulation::{
    NpcEffect, NpcNotification, NpcProposal, NpcServiceAction as A, NpcServiceResult as R,
    PlayerSnapshotOutcome,
};
use bace_types::EntityId;
use rand_core::{OsRng, RngCore};
use std::collections::BTreeSet;
type DeletionPrepared = (
    EntityId,
    u64,
    Result<Vec<crate::game_inventory::FrozenInventoryItem>, String>,
);
type GiftIdsPrepared = (
    EntityId,
    u64,
    crate::npc_items::NpcGivePlan,
    Result<Vec<u32>, String>,
);
type SourceHeadPrepared = (
    PreparedNpcRegistration,
    Result<
        (
            Option<bace_db_postgres::StoredNpcSourceHead>,
            Option<bace_persistence::StoredAggregate>,
        ),
        String,
    >,
);

pub(super) struct NpcRuntime {
    generated_turn: bool,
    head_registration: Option<PreparedNpcRegistration>,
    generated_registrations: VecDeque<PreparedNpcRegistration>,
    regions: BTreeMap<u64, (EntityId, u16)>,
    source_inventory: BTreeMap<EntityId, Arc<crate::npc_persistence::FrozenNpcSourceInventory>>,
    coordinator: NpcCoordinator,
    deletion_job: Option<Job<DeletionPrepared>>,
    motion_worker: Option<crate::npc_motion_assets::NpcMotionWorker>,
    motion_unexpected: Option<crate::npc_motion_assets::NpcMotionResult>,
    item_worker: Option<crate::npc_items::NpcItemWorker>,
    item_unexpected: Option<crate::npc_items::NpcItemCompletion>,
    gift_ids: Option<Job<GiftIdsPrepared>>,
    inventory_output: VecDeque<inventory_output::Delivery>,
    server_magic: Option<crate::server_magic_assets::ServerMagicWorker>,
    server_magic_unexpected: Option<crate::server_magic_assets::ServerMagicResult>,
    initialized: bool,
    pending_recovery: VecDeque<EntityId>,
    recovery_ids: BTreeSet<EntityId>,
    recovery_job: Option<Job<Result<NpcRecoveryRequest, String>>>,
    recovery_request: Option<NpcRecoveryRequest>,
    recovery_worker: Option<NpcRecoveryWorker>,
    recovery_inflight: usize,
    prepared_recovery: Option<PreparedNpcRecovery>,
    registration: Option<PreparedNpcRegistration>,
    head_job: Option<Job<SourceHeadPrepared>>,
    definitions: BTreeMap<EntityId, PreparedNpcRegistration>,
    proposals: VecDeque<NpcProposal>,
    notifications: VecDeque<NpcNotification>,
    notifications_drained: bool,
    retired: VecDeque<EntityId>,
    handins: BTreeMap<EntityId, handin::HandIn>,
    uses: BTreeMap<EntityId, handin::Use>,
    work: BTreeMap<EntityId, effects::Work>,
    idle: BTreeMap<EntityId, terminal::Idle>,
    terminal_cursor: usize,
    effect_cursor: Option<EntityId>,
    shared: Option<(
        bace_simulation::AllegianceTicket,
        Option<bace_simulation::NpcSourceCheckpoint>,
    )>,
    unexpected: Option<Arc<bace_simulation::NpcServiceOutcome>>,
    held_event: Option<NpcCoordinatorEvent>,
    failure: Option<String>,
}
impl NpcRuntime {
    pub(super) fn vendor_registration(&self, id: EntityId) -> Option<&PreparedNpcRegistration> {
        self.definitions
            .get(&id)
            .filter(|source| source.admitted && source.source.authored.weenie_type == 12)
    }

    pub(super) fn vendor_source_pending(&self, id: EntityId) -> bool {
        self.coordinator.needs_terminal(id)
            || self.coordinator.busy(id)
            || self.idle.contains_key(&id)
    }

    fn committed_source_receipt(&self, event: &NpcCoordinatorEvent) -> bool {
        use crate::npc_persistence::{
            NpcCheckpointResolution as C, NpcHandInResolution as H, NpcStageResolution as S,
        };
        let source = match event {
            NpcCoordinatorEvent::Checkpoint {
                source,
                resolution: C::Committed,
            } => *source,
            NpcCoordinatorEvent::Durable { source, resolution }
                if matches!(resolution.as_ref(), S::Committed { .. }) =>
            {
                *source
            }
            NpcCoordinatorEvent::HandIn { source, resolution }
                if matches!(resolution.as_ref(), H::Committed { .. }) =>
            {
                *source
            }
            _ => return false,
        };
        self.source_inventory.get(&source).is_some_and(|frozen| {
            !frozen.snapshots().is_empty()
                || (frozen.root_snapshot().is_some()
                    && frozen.root_is_item()
                    && !frozen.root_is_static_shop())
        })
    }

    pub(super) fn new() -> Self {
        Self {
            generated_turn: false,
            head_registration: None,
            generated_registrations: VecDeque::new(),
            regions: BTreeMap::new(),
            source_inventory: BTreeMap::new(),
            coordinator: NpcCoordinator::new(4096).expect("fixed NPC bound"),
            deletion_job: None,
            motion_worker: None,
            motion_unexpected: None,
            item_worker: None,
            item_unexpected: None,
            gift_ids: None,
            inventory_output: VecDeque::new(),
            server_magic: None,
            server_magic_unexpected: None,
            initialized: false,
            pending_recovery: VecDeque::new(),
            recovery_ids: BTreeSet::new(),
            recovery_job: None,
            recovery_request: None,
            recovery_worker: None,
            recovery_inflight: 0,
            prepared_recovery: None,
            registration: None,
            head_job: None,
            definitions: BTreeMap::new(),
            proposals: VecDeque::new(),
            notifications: VecDeque::new(),
            notifications_drained: false,
            retired: VecDeque::new(),
            handins: BTreeMap::new(),
            uses: BTreeMap::new(),
            work: BTreeMap::new(),
            idle: BTreeMap::new(),
            terminal_cursor: 0,
            effect_cursor: None,
            shared: None,
            unexpected: None,
            held_event: None,
            failure: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.generated_registrations.is_empty()
            || !self.regions.is_empty()
            || !self.source_inventory.is_empty()
            || self.deletion_job.is_some()
            || self.motion_unexpected.is_some()
            || !self.inventory_output.is_empty()
            || self.item_unexpected.is_some()
            || self.gift_ids.is_some()
            || self.server_magic_unexpected.is_some()
            || !self.uses.is_empty()
            || !self.handins.is_empty()
            || !self.idle.is_empty()
            || self.shared.is_some()
            || !self.retired.is_empty()
            || self.coordinator.has_pending()
            || self.recovery_inflight != 0
            || !self.pending_recovery.is_empty()
            || self.recovery_job.is_some()
            || self.recovery_request.is_some()
            || self.prepared_recovery.is_some()
            || self.registration.is_some()
            || self.head_job.is_some()
            || !self.work.is_empty()
            || !self.proposals.is_empty()
            || !self.notifications.is_empty()
            || self.unexpected.is_some()
            || self.held_event.is_some()
    }
}
impl GameRuntime {
    pub(super) fn prepare_npc_shared(
        &mut self,
        ticket: &bace_simulation::AllegianceTicket,
    ) -> Result<Option<crate::allegiance_service::NpcSharedCheckpoint>, String> {
        let proposal = ticket.npc.as_ref().ok_or("NPC shared proposal missing")?;
        let source = proposal.context.source;
        if let Some((held, checkpoint)) = &mut self.npc.shared {
            if held != ticket {
                return Err("NPC shared reward owner conflict".into());
            }
            if let Some(checkpoint) = checkpoint.take() {
                let source_inventory = match source_inventory::prepare(
                    &mut self.npc.source_inventory,
                    self.world.as_ref().map(|w| &w.regions),
                    &self.npc.definitions,
                    checkpoint.inventory.as_ref(),
                    self.bootstrap.world_owner.epoch(),
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.npc.shared.as_mut().expect("held shared stage").1 = Some(checkpoint);
                        return Err(error);
                    }
                };
                let token = self
                    .npc
                    .coordinator
                    .reserve_external(source, ticket.operation)?;
                self.npc.shared = None;
                return Ok(Some(crate::allegiance_service::NpcSharedCheckpoint {
                    source_inventory,
                    token,
                    checkpoint,
                }));
            }
            return Ok(None);
        }
        if self.npc.handins.contains_key(&source)
            || self.npc.work.contains_key(&source)
            || self.npc.coordinator.busy(source)
        {
            return Ok(None);
        }
        self.npc
            .coordinator
            .enqueue(
                source,
                A::PreviewOwnerCompletion {
                    proposal: proposal.clone(),
                },
            )
            .map_err(|_| "NPC shared checkpoint admission held")?;
        self.npc.shared = Some((ticket.clone(), None));
        Ok(None)
    }
    pub(super) fn finish_npc_shared(
        &mut self,
        ticket: crate::npc_service::NpcExternalStage,
        committed: bool,
    ) -> Result<(), String> {
        let source = EntityId(ticket.binding.source);
        if let Some(frozen) = self.npc.source_inventory.get(&source)
            && committed
        {
            let world = self
                .world
                .as_mut()
                .ok_or("NPC shared source receipt owner")?;
            world.regions.adopt_npc_item_snapshots(frozen.snapshots())?;
            if let Some(root) = frozen.root_snapshot() {
                if frozen.root_is_item() && !frozen.root_is_static_shop() {
                    world
                        .regions
                        .adopt_npc_item_snapshots(std::slice::from_ref(root))?;
                }
                self.npc
                    .definitions
                    .get_mut(&source)
                    .ok_or("NPC shared root baseline owner")?
                    .baseline = Some(bace_persistence::StoredAggregate {
                    object_id: root.object_id,
                    persisted_version: root.expected_version + 1,
                    bytes: root.bytes.clone(),
                });
            }
        }
        self.npc.coordinator.finish_external(ticket, committed)?;
        self.npc.source_inventory.remove(&source);
        Ok(())
    }

    pub fn npc_failure(&self) -> Option<&str> {
        self.npc.failure.as_deref()
    }
    pub fn pending_npc_notification(&self) -> Option<&NpcNotification> {
        self.npc.notifications.front()
    }
    /// Output routing must own the complete exact notification before removing
    /// it. Broadcast audience derives from accepted visibility, never sessions.
    pub fn acknowledge_npc_notification(
        &mut self,
        expected: &NpcNotification,
    ) -> Result<(), String> {
        if self.npc.notifications.front() != Some(expected) {
            return Err("NPC notification receipt mismatch".into());
        }
        self.npc.notifications.pop_front();
        Ok(())
    }
    pub(super) fn npc_owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.npc
            .handins
            .values()
            .any(|w| w.capture == Some(outcome.correlation))
            || self
                .npc
                .work
                .values()
                .any(|w| w.capture == Some(outcome.correlation))
    }
    pub(super) fn accept_npc_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if let Some(work) = self
            .npc
            .handins
            .values_mut()
            .find(|w| w.capture == Some(outcome.correlation))
        {
            work.capture = None;
            match outcome.result {
                Ok(snapshot) => work.snapshot = Some((snapshot, unix)),
                Err(error) => self.npc.failure = Some(format!("NPC hand-in capture: {error:?}")),
            };
            return Ok(());
        }
        let Some(work) = self
            .npc
            .work
            .values_mut()
            .find(|w| w.capture == Some(outcome.correlation))
        else {
            return Err(outcome);
        };
        work.capture = None;
        match outcome.result {
            Ok(snapshot) => work.snapshot = Some((snapshot, unix)),
            Err(error) => self.npc.failure = Some(format!("NPC player capture: {error:?}")),
        };
        Ok(())
    }
    pub(super) fn poll_npcs(&mut self, _elapsed: Duration) -> Result<(), String> {
        if let Some(world) = &mut self.world {
            while let Some(&source) = self.npc.retired.front() {
                world.regions.note_npc_archive(source)?;
                self.npc.retired.pop_front();
            }
        }
        let mut failure = self.poll_npc_sources().err();
        if let Err(error) = self.poll_npc_inventory_output() {
            failure.get_or_insert(error);
        }
        if let Some(outcome) = self.npc.unexpected.take()
            && let Err(outcome) = self.npc.coordinator.accept(outcome)
        {
            self.npc.unexpected = Some(outcome);
            failure.get_or_insert("NPC outcome correlation/pressure retained".into());
        }
        if self.npc.unexpected.is_none() {
            for _ in 0..self.limits.work_per_poll {
                let Ok(outcome) = self.simulation.npc_service_outcomes().try_recv() else {
                    break;
                };
                if let Err(outcome) = self.npc.coordinator.accept(outcome) {
                    self.npc.unexpected = Some(outcome);
                    break;
                }
            }
        }
        for _ in 0..self.limits.work_per_poll {
            if self.npc.held_event.is_none() {
                self.npc.held_event = self.npc.coordinator.take_event();
            }
            let Some(event) = self.npc.held_event.take() else {
                break;
            };
            // Entry temporarily lends the region owner to activation. A
            // committed NPC checkpoint may arrive during that handoff, but
            // its joined V5 rows must reach the region cache and root
            // baseline before the coordinator can consume the event.
            if self.world.is_none() && self.npc.committed_source_receipt(&event) {
                self.npc.held_event = Some(event);
                break;
            }
            if let Err(error) = self.accept_npc_source_inventory(&event) {
                self.npc.held_event = Some(event);
                failure.get_or_insert(error);
                break;
            }
            if let Err((error, event)) =
                effects::accept(&mut self.npc, &mut self.online_saves, event)
            {
                self.npc.held_event = Some(event);
                self.npc.failure = Some(error.clone());
                failure.get_or_insert(error);
                break;
            }
        }
        while self.npc.proposals.len() < 256 {
            let Ok(proposal) = self.simulation.npc_proposals().try_recv() else {
                break;
            };
            self.npc
                .coordinator
                .observe_activity(proposal.context.source);
            self.npc.proposals.push_back(proposal);
        }
        self.npc.notifications_drained = false;
        while self.npc.notifications.len() < 256 {
            let Ok(notification) = self.simulation.npc_notifications().try_recv() else {
                self.npc.notifications_drained = true;
                break;
            };
            self.npc.notifications.push_back(notification);
        }
        for _ in 0..self.npc.proposals.len().min(self.limits.work_per_poll) {
            let proposal = self.npc.proposals.pop_front().expect("bounded NPC queue");
            let source = proposal.context.source;
            if self.npc.handins.contains_key(&source)
                || self.npc.work.contains_key(&source)
                || self.npc.coordinator.busy(source)
            {
                self.npc.proposals.push_back(proposal);
                continue;
            }
            if self.npc.coordinator.binding(source).is_none() {
                self.npc.proposals.push_back(proposal);
                failure.get_or_insert("NPC proposal has no pinned source binding".into());
                continue;
            }
            self.npc.work.insert(source, effects::Work::new(proposal));
        }
        if let Some((source, ticket, result)) = ready(&mut self.npc.deletion_job) {
            let work = self
                .npc
                .work
                .get_mut(&source)
                .ok_or("NPC deletion source missing")?;
            if work.ticket() != ticket {
                return Err("NPC deletion preparation ticket mismatch".into());
            }
            work.set_deletion_sources(result)?;
        }
        for result in [
            self.poll_npc_item_worker(),
            self.poll_npc_motion_worker(),
            self.poll_npc_cast_worker(),
            self.poll_npc_handins(),
            self.poll_npc_effects(),
            self.poll_npc_terminals(),
        ] {
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        self.npc
            .coordinator
            .pump(&self.simulation.input(), &self.saves.handle);
        failure.map_or(Ok(()), Err)
    }
    fn poll_npc_sources(&mut self) -> Result<(), String> {
        if !self.npc.initialized {
            let Some(world) = &self.world else {
                return Ok(());
            };
            let (pending, _) = world.regions.npc_recovery_inventory();
            self.npc.pending_recovery = pending.iter().copied().collect();
            self.npc.recovery_ids = (*pending).clone();
            self.npc.initialized = true;
        }
        if self.npc.recovery_worker.is_none() && !self.npc.pending_recovery.is_empty() {
            self.npc.recovery_worker = Some(NpcRecoveryWorker::start(
                self.bootstrap
                    .config
                    .pack_directory
                    .clone()
                    .ok_or("NPC history requires pack directory")?,
                2,
            )?);
        }
        if let Some(request) = ready(&mut self.npc.recovery_job) {
            self.npc.recovery_request = Some(request?);
        }
        if self.npc.recovery_request.is_none()
            && self.npc.recovery_job.is_none()
            && let Some(&source) = self.npc.pending_recovery.front()
        {
            let store = self.bootstrap.store.clone();
            self.npc.recovery_job = Some(Box::pin(async move {
                crate::npc_recovery::load_recovery_request(&store, source).await
            }));
        }
        if let Some(request) = self.npc.recovery_request.take() {
            match self
                .npc
                .recovery_worker
                .as_ref()
                .ok_or("NPC recovery worker missing")?
                .try_submit(request)
            {
                Ok(()) => {
                    self.npc.pending_recovery.pop_front();
                    self.npc.recovery_inflight += 1;
                }
                Err(request) => self.npc.recovery_request = Some(*request),
            }
        }
        if self.npc.prepared_recovery.is_none()
            && let Some(worker) = &self.npc.recovery_worker
            && let Ok(result) = worker.try_recv()
        {
            self.npc.recovery_inflight -= 1;
            match result.result {
                Ok(value) => self.npc.prepared_recovery = Some(value),
                Err(error) => {
                    self.npc.pending_recovery.push_front(result.source);
                    return Err(error);
                }
            }
        }
        if let Some(recovery) = self.npc.prepared_recovery.take() {
            let source = EntityId(recovery.binding.source);
            if !recovery.archived() && !self.npc.definitions.contains_key(&source) {
                self.npc.prepared_recovery = Some(recovery);
            } else {
                let radius = match recovery
                    .source
                    .properties
                    .get(bace_entity::PropertyFamily::Float, 54)
                {
                    Some(bace_entity::PropertyValue::Float(v)) => *v as f32,
                    _ => 0.6,
                };
                match self.npc.coordinator.recover_with_registry(
                    recovery,
                    radius,
                    self.npc
                        .definitions
                        .get(&source)
                        .and_then(|d| d.object_registry.clone()),
                    self.npc
                        .definitions
                        .get(&source)
                        .filter(|d| d.admitted)
                        .map(|d| d.script_identity()),
                ) {
                    Ok(()) => {
                        self.npc.recovery_ids.remove(&source);
                    }
                    Err((error, recovery)) => {
                        self.npc.prepared_recovery = Some(*recovery);
                        return Err(error);
                    }
                }
            }
        }
        if let Some((mut registration, result)) = ready(&mut self.npc.head_job) {
            self.npc.head_registration = None;
            let head = match result {
                Ok((head, baseline)) => {
                    registration.baseline = baseline;
                    head
                }
                Err(error) => {
                    self.npc.registration = Some(registration);
                    return Err(error);
                }
            };
            if head.as_ref().is_some_and(|h| !h.completed) {
                self.npc.registration = Some(registration);
                return Err("unloaded NPC canonical recovery head".into());
            }
            let completed_state = (|| -> Result<_, String> {
                let Some(head) = &head else {
                    return Ok(None);
                };
                let checkpoint =
                    bace_storage_codec::NpcWorkflowSaveV3::decode_or_migrate(&head.checkpoint)
                        .map_err(|e| e.to_string())?;
                if checkpoint.archive.is_some()
                    || checkpoint.source_template != registration.source.template
                {
                    return Err("NPC completed source suppression/template conflict".into());
                }
                let saved_binding = NpcCheckpointBinding {
                    source_version: checkpoint.source_version,
                    source_template: checkpoint.source_template,
                    source: checkpoint.source,
                    invocation: checkpoint.invocation,
                    program_hash: checkpoint.program_hash,
                    content_generation: checkpoint.content_generation,
                };
                crate::npc_persistence::restore_checkpoint(saved_binding, checkpoint)
                    .map(Some)
                    .map_err(|e| e.to_string())
            })();
            let completed_state = match completed_state {
                Ok(value) => value,
                Err(error) => {
                    self.npc.registration = Some(registration);
                    return Err(error);
                }
            };
            let mut invocation = [0; 16];
            OsRng.fill_bytes(&mut invocation);
            let binding = NpcCheckpointBinding {
                source_version: head.as_ref().map_or(0, |h| h.source_version),
                source_template: registration.source.template,
                source: registration.actor.0,
                invocation,
                program_hash: registration.source.program_hash,
                content_generation: registration.content_hash,
            };
            self.npc.coordinator.bind(binding, 0)?;
            if let Some(snapshot) = completed_state {
                self.npc
                    .coordinator
                    .register_completed_source(&registration, snapshot)?;
            } else if registration.admitted {
                let static_shop = crate::npc_persistence::is_authored_static_shop(&registration)?;
                self.npc
                    .coordinator
                    .activate_admitted_source(&registration)?;
                // A fresh authored Shop has no script activity to request the
                // terminal checkpoint that gives its stock a durable world
                // source. Reuse that held NPC owner before its first Use.
                if static_shop {
                    self.npc.coordinator.observe_activity(registration.actor);
                }
            } else {
                self.npc
                    .coordinator
                    .enqueue_retained(
                        registration.actor,
                        A::Register {
                            actor: registration.actor,
                            program: registration.source.program.clone(),
                            use_radius: registration.use_radius,
                            properties: Some(registration.source.properties.clone()),
                        },
                    )
                    .map_err(|_| "NPC registration delivery held")?;
            }
            self.npc
                .definitions
                .insert(registration.actor, registration);
        }
        if self.npc.registration.is_none()
            && self.npc.head_job.is_none()
            && let Some(world) = &mut self.world
        {
            self.npc.generated_turn = !self.npc.generated_turn;
            self.npc.registration = if self.npc.generated_turn {
                self.npc
                    .generated_registrations
                    .pop_front()
                    .or_else(|| world.regions.take_npc_source())
            } else {
                world
                    .regions
                    .take_npc_source()
                    .or_else(|| self.npc.generated_registrations.pop_front())
            };
        }
        if let Some(registration) = self.npc.registration.take() {
            if self.npc.recovery_ids.contains(&registration.actor) {
                self.npc
                    .definitions
                    .insert(registration.actor, registration);
                return Ok(());
            }
            if self.npc.coordinator.binding(registration.actor).is_some() {
                let existing = self
                    .npc
                    .definitions
                    .get(&registration.actor)
                    .ok_or("NPC bound source definition missing")?;
                if existing.epoch == registration.epoch
                    && existing.script_identity() == registration.script_identity()
                {
                    return Ok(());
                }
                if registration.epoch <= existing.epoch
                    || self
                        .npc
                        .notifications
                        .iter()
                        .any(|n| n.context.source == registration.actor)
                    || self
                        .npc
                        .coordinator
                        .release_source(registration.actor)
                        .is_err()
                {
                    self.npc.registration = Some(registration);
                    return Err("NPC previous source incarnation remains retained".into());
                }
                self.npc.definitions.remove(&registration.actor);
            }
            self.npc.head_registration = Some(registration.clone());
            let store = self.bootstrap.store.clone();
            self.npc.head_job = Some(Box::pin(async move {
                let result = async {
                    let head = store
                        .npc_source_head(registration.actor.0)
                        .await
                        .map_err(|e| e.to_string())?;
                    let baseline = store
                        .load(registration.actor.0)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok((head, baseline))
                }
                .await;
                (registration, result)
            }));
        }
        Ok(())
    }
}
