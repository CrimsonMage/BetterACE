//! Bounded, per-source service delivery. Frozen writes survive pressure and
//! uncertainty; canonical source versions advance only from exact save receipts.
mod recovery;
use crate::{
    npc_persistence::{
        NpcCheckpointBinding, NpcHandInResolution, NpcStageResolution, PendingNpcHandIn,
        PendingNpcStage,
    },
    saves::SaveHandle,
    simulation::SimulationInput,
};
use bace_simulation::{
    Command, InventoryReceipt, NpcDeleteSourceTicket, NpcInventoryTicket, NpcPortalTicket,
    NpcProposal, NpcServiceAction as A, NpcServiceCommand, NpcServiceOutcome, NpcSkillResetTicket,
    NpcSpellbookTicket, NpcTrainingCreditTicket,
};
use bace_storage_codec::NpcWorkflowSaveV3;
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

/// Typed ownership needed after durable acceptance. Generic replies cannot
/// release these service tickets; each uses its named simulation owner.
#[derive(Clone)]
pub enum NpcDurableAdoption {
    Cast {
        proposal: NpcProposal,
        outcome: bace_gameplay_api::ServerCastOutcome,
    },
    Motion {
        proposal: NpcProposal,
        event: bace_world::WorldMotionEvent,
    },
    Player,
    Inventory {
        ticket: NpcInventoryTicket,
        receipt: InventoryReceipt,
    },
    Spellbook(NpcSpellbookTicket),
    Credits(NpcTrainingCreditTicket),
    SkillReset(NpcSkillResetTicket),
    Teleport {
        ticket: NpcPortalTicket,
        receipt: bace_simulation::PortalServiceReceipt,
    },
    Delete {
        ticket: NpcDeleteSourceTicket,
        receipt: Option<InventoryReceipt>,
    },
}
impl NpcDurableAdoption {
    fn proposal(&self) -> Option<&NpcProposal> {
        match self {
            Self::Player => None,
            Self::Cast { proposal, .. } | Self::Motion { proposal, .. } => Some(proposal),
            Self::Inventory { ticket, .. } => Some(&ticket.npc),
            Self::Spellbook(t) => Some(&t.npc),
            Self::Credits(t) => Some(&t.npc),
            Self::SkillReset(t) => Some(&t.npc),
            Self::Teleport { ticket, .. } => Some(&ticket.npc),
            Self::Delete { ticket, .. } => Some(&ticket.npc),
        }
    }
    fn commit(
        &self,
        proposal: NpcProposal,
        adoption: crate::npc_persistence::NpcStageAdoption,
    ) -> A {
        use crate::npc_persistence::NpcStageAdoption as S;
        match adoption {
            S::QueuedExperience => return A::AdmitExperience { proposal },
            S::DetachedService { post_delay } => {
                return A::AdmitService {
                    proposal,
                    post_delay,
                };
            }
            S::Effect(_) => {}
        }
        match self {
            Self::Player => A::CommitPlayerEffect { proposal },
            Self::Cast { outcome, .. } => A::CommitCast {
                proposal,
                outcome: outcome.clone(),
            },
            Self::Motion { event, .. } => A::CommitMotion {
                proposal,
                event: *event,
            },
            Self::Inventory { ticket, receipt } => A::CommitInventory {
                ticket: ticket.clone(),
                receipt: receipt.clone(),
            },
            Self::Spellbook(ticket) => A::CommitSpellbook {
                ticket: ticket.clone(),
            },
            Self::Credits(ticket) => A::CommitTrainingCredits {
                ticket: ticket.clone(),
            },
            Self::SkillReset(ticket) => A::CommitSkillReset {
                ticket: ticket.clone(),
            },
            Self::Teleport { ticket, receipt } => A::CommitTeleport {
                ticket: ticket.clone(),
                receipt: receipt.clone(),
            },
            Self::Delete {
                ticket,
                receipt: Some(receipt),
            } => A::CommitRetirement {
                ticket: ticket.clone(),
                receipt: receipt.clone(),
            },
            Self::Delete {
                ticket,
                receipt: None,
            } => A::CommitTransientDeletion {
                ticket: ticket.clone(),
            },
        }
    }
    fn reject(&self, proposal: &NpcProposal) -> Option<A> {
        Some(match self {
            Self::Cast { proposal, .. } | Self::Motion { proposal, .. } => A::ReleaseJournal {
                proposal: proposal.clone(),
            },
            Self::Player => A::ReleaseJournal {
                proposal: proposal.clone(),
            },
            Self::Inventory { ticket, .. } => A::RejectInventory {
                ticket: ticket.clone(),
            },
            Self::Spellbook(ticket) => A::RejectSpellbook {
                ticket: ticket.clone(),
            },
            Self::Credits(ticket) => A::RejectTrainingCredits {
                ticket: ticket.clone(),
            },
            Self::SkillReset(ticket) => A::RejectSkillReset {
                ticket: ticket.clone(),
            },
            Self::Teleport { ticket, .. } => A::RejectTeleport {
                ticket: ticket.clone(),
            },
            Self::Delete { ticket, .. } => A::RejectDeletion {
                ticket: ticket.clone(),
            },
        })
    }
}
pub enum NpcCoordinatorEvent {
    Checkpoint {
        source: EntityId,
        resolution: crate::npc_persistence::NpcCheckpointResolution,
    },
    Held {
        source: EntityId,
        message: String,
    },
    Command {
        source: EntityId,
        outcome: Arc<NpcServiceOutcome>,
    },
    Durable {
        source: EntityId,
        resolution: Box<NpcStageResolution>,
    },
    HandIn {
        source: EntityId,
        resolution: Box<NpcHandInResolution>,
    },
}
struct Delivery {
    command: NpcServiceCommand,
    submitted: bool,
    retry_on_failure: bool,
}
enum Write {
    Checkpoint {
        pending: Box<crate::npc_persistence::PendingNpcCheckpoint>,
        operation: u64,
    },
    Stage {
        pending: Box<PendingNpcStage>,
        adoption: Box<NpcDurableAdoption>,
    },
    HandIn(Box<PendingNpcHandIn>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcExternalStage {
    pub(crate) binding: NpcCheckpointBinding,
    pub(crate) workflow_version: i64,
    pub(crate) operation: u64,
}
struct Source {
    completed: bool,
    external: Option<u64>,
    binding: NpcCheckpointBinding,
    workflow_version: i64,
    delivery: Option<Delivery>,
    write: Option<Write>,
    write_submitted: bool,
    write_correlation: u64,
    recovery: VecDeque<NpcServiceCommand>,
    recovering: bool,
    generation: Option<Arc<bace_storage_codec::PackGeneration>>,
    retained_bytes: usize,
    submission_failure: Option<String>,
}
pub struct NpcCoordinator {
    capacity: usize,
    next_correlation: u64,
    sources: BTreeMap<EntityId, Source>,
    events: VecDeque<NpcCoordinatorEvent>,
}
impl NpcCoordinator {
    pub fn new(capacity: usize) -> Result<Self, String> {
        if !(1..=4096).contains(&capacity) {
            return Err("NPC coordinator capacity".into());
        }
        Ok(Self {
            capacity,
            next_correlation: 1,
            sources: BTreeMap::new(),
            events: VecDeque::new(),
        })
    }
    /// Bind after canonical head load (including completed heads for a new
    /// invocation). Only the exact durable receipt may advance this version.
    pub fn bind(
        &mut self,
        binding: NpcCheckpointBinding,
        workflow_version: i64,
    ) -> Result<(), String> {
        let source = EntityId(binding.source);
        if source.0 == 0
            || binding.source_template == 0
            || workflow_version < 0
            || self.sources.contains_key(&source)
            || self.sources.len() == self.capacity
        {
            return Err("NPC source binding/conflict/capacity".into());
        }
        self.sources.insert(
            source,
            Source {
                completed: workflow_version == 0,
                external: None,
                binding,
                workflow_version,
                delivery: None,
                write: None,
                write_submitted: false,
                write_correlation: 0,
                recovery: VecDeque::new(),
                recovering: false,
                generation: None,
                retained_bytes: 0,
                submission_failure: None,
            },
        );
        Ok(())
    }
    /// Live source bodies/properties must already have been admitted by their
    /// actual world owner. Archived sources register only their descriptor.
    pub(crate) fn reserve_external(
        &mut self,
        source: EntityId,
        operation: u64,
    ) -> Result<NpcExternalStage, String> {
        if operation == 0 || self.busy(source) {
            return Err("NPC source occupied by another stage".into());
        }
        let state = self
            .sources
            .get_mut(&source)
            .ok_or("NPC shared source missing")?;
        if state.binding.source_version >= i64::MAX as u64 || state.workflow_version == i64::MAX {
            return Err("NPC shared source version overflow".into());
        }
        state.external = Some(operation);
        Ok(NpcExternalStage {
            binding: state.binding,
            workflow_version: state.workflow_version,
            operation,
        })
    }
    pub(crate) fn finish_external(
        &mut self,
        ticket: NpcExternalStage,
        committed: bool,
    ) -> Result<(), String> {
        let state = self
            .sources
            .get_mut(&EntityId(ticket.binding.source))
            .ok_or("NPC external stage source missing")?;
        if state.external != Some(ticket.operation)
            || state.binding != ticket.binding
            || state.workflow_version != ticket.workflow_version
        {
            return Err("NPC external stage receipt mismatch".into());
        }
        if committed {
            state.completed = false;
            state.binding.source_version += 1;
            state.workflow_version += 1;
        }
        state.external = None;
        Ok(())
    }
    pub fn binding(&self, source: EntityId) -> Option<(NpcCheckpointBinding, i64)> {
        self.sources
            .get(&source)
            .map(|s| (s.binding, s.workflow_version))
    }
    pub fn busy(&self, source: EntityId) -> bool {
        self.sources.get(&source).is_some_and(|s| {
            s.external.is_some() || s.recovering || s.delivery.is_some() || s.write.is_some()
        })
    }
    pub fn has_pending(&self) -> bool {
        !self.events.is_empty()
            || self.sources.values().any(|s| {
                s.external.is_some() || s.recovering || s.delivery.is_some() || s.write.is_some()
            })
    }
    pub fn enqueue(&mut self, source: EntityId, action: A) -> Result<(), Box<A>> {
        let Some(state) = self.sources.get_mut(&source) else {
            return Err(Box::new(action));
        };
        if state.external.is_some()
            || state.recovering
            || state.delivery.is_some()
            || state.write.is_some()
        {
            return Err(Box::new(action));
        }
        let Some(next) = self.next_correlation.checked_add(1) else {
            return Err(Box::new(action));
        };
        let command = NpcServiceCommand {
            correlation: self.next_correlation,
            action,
        };
        if !command.valid_bounds() {
            return Err(Box::new(command.action));
        }
        state.delivery = Some(Delivery {
            command,
            submitted: false,
            retry_on_failure: false,
        });
        self.next_correlation = next;
        Ok(())
    }

    pub(crate) fn enqueue_retained(&mut self, source: EntityId, action: A) -> Result<(), Box<A>> {
        self.enqueue(source, action)?;
        self.sources
            .get_mut(&source)
            .and_then(|s| s.delivery.as_mut())
            .expect("enqueued source command")
            .retry_on_failure = true;
        Ok(())
    }
    /// Returns the original frozen stage on every rejected admission.
    fn write_room(&self, operation: &bace_persistence::NpcStageOperation) -> bool {
        let size = |o: &bace_persistence::NpcStageOperation| {
            o.inventory
                .snapshots
                .iter()
                .try_fold(o.workflow.checkpoint.len(), |n, s| {
                    n.checked_add(s.bytes.len())
                })
        };
        let mut bytes = size(operation);
        for source in self.sources.values() {
            bytes = bytes.and_then(|n| n.checked_add(source.retained_bytes));
            if let Some(write) = &source.write {
                let operation = match write {
                    Write::Stage { pending, .. } => pending.operation(),
                    Write::HandIn(pending) => pending.operation(),
                    Write::Checkpoint { pending, .. } => pending.operation(),
                };
                bytes = bytes.and_then(|n| n.checked_add(size(operation)?));
            }
        }
        bytes.is_some_and(|n| n <= 64 * 1024 * 1024)
    }
    pub fn enqueue_stage(
        &mut self,
        source: EntityId,
        pending: PendingNpcStage,
        adoption: NpcDurableAdoption,
    ) -> Result<(), Box<PendingNpcStage>> {
        if !self.write_room(pending.operation()) {
            return Err(Box::new(pending));
        }
        let Some(state) = self.sources.get_mut(&source) else {
            return Err(Box::new(pending));
        };
        if state.external.is_some()
            || state.recovering
            || state.delivery.is_some()
            || state.write.is_some()
            || !stage_matches(state, pending.operation())
            || adoption.proposal().is_some_and(|p| p != pending.proposal())
        {
            return Err(Box::new(pending));
        }
        let Some(next) = self.next_correlation.checked_add(1) else {
            return Err(Box::new(pending));
        };
        state.write_correlation = self.next_correlation;
        self.next_correlation = next;
        state.write = Some(Write::Stage {
            pending: Box::new(pending),
            adoption: Box::new(adoption),
        });
        Ok(())
    }
    pub(crate) fn source_ready(&self, source: EntityId) -> bool {
        self.sources.get(&source).is_some_and(|s| !s.recovering)
    }
    pub(crate) fn observe_activity(&mut self, source: EntityId) {
        if let Some(state) = self.sources.get_mut(&source) {
            state.completed = false;
        }
    }
    pub fn needs_terminal(&self, source: EntityId) -> bool {
        self.source_ready(source)
            && self.sources.get(&source).is_some_and(|s| !s.completed)
            && !self.busy(source)
    }
    pub(crate) fn terminal_candidates(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.sources
            .keys()
            .copied()
            .filter(|id| self.needs_terminal(*id))
    }
    pub(crate) fn begin_invocation(
        &mut self,
        source: EntityId,
        event: [u8; 16],
    ) -> Result<(), String> {
        if self.busy(source) || event == [0; 16] {
            return Err("NPC source invocation busy".into());
        }
        let state = self.sources.get_mut(&source).ok_or("NPC source missing")?;
        if !state.completed {
            return Err("NPC previous source head is not complete".into());
        }
        state.binding.invocation = event;
        state.workflow_version = 0;
        Ok(())
    }
    pub(crate) fn enqueue_checkpoint(
        &mut self,
        source: EntityId,
        operation: u64,
        pending: crate::npc_persistence::PendingNpcCheckpoint,
    ) -> Result<(), Box<crate::npc_persistence::PendingNpcCheckpoint>> {
        if operation == 0 || !self.write_room(pending.operation()) || self.busy(source) {
            return Err(Box::new(pending));
        }
        let Some(state) = self.sources.get_mut(&source) else {
            return Err(Box::new(pending));
        };
        if !stage_matches(state, pending.operation()) {
            return Err(Box::new(pending));
        }
        let Some(next) = self.next_correlation.checked_add(1) else {
            return Err(Box::new(pending));
        };
        state.write_correlation = self.next_correlation;
        self.next_correlation = next;
        state.write = Some(Write::Checkpoint {
            pending: Box::new(pending),
            operation,
        });
        Ok(())
    }
    pub fn enqueue_handin(
        &mut self,
        source: EntityId,
        pending: PendingNpcHandIn,
    ) -> Result<(), Box<PendingNpcHandIn>> {
        if !self.write_room(pending.operation()) {
            return Err(Box::new(pending));
        }
        let Some(state) = self.sources.get_mut(&source) else {
            return Err(Box::new(pending));
        };
        if state.external.is_some()
            || state.recovering
            || state.delivery.is_some()
            || state.write.is_some()
            || !stage_matches(state, pending.operation())
        {
            return Err(Box::new(pending));
        }
        let Some(next) = self.next_correlation.checked_add(1) else {
            return Err(Box::new(pending));
        };
        state.write_correlation = self.next_correlation;
        self.next_correlation = next;
        state.write = Some(Write::HandIn(Box::new(pending)));
        Ok(())
    }
    /// Every pass is bounded by configured sources. Full save/simulation/event
    /// queues retain ownership and prevent another stage for that source.
    pub fn pump(&mut self, input: &SimulationInput, saves: &SaveHandle) {
        for (&source, state) in &mut self.sources {
            if self.events.len() == self.capacity {
                break;
            }
            if state.delivery.is_none()
                && let Some(command) = state.recovery.pop_front()
            {
                state.delivery = Some(Delivery {
                    command,
                    submitted: false,
                    retry_on_failure: true,
                });
            }
            if let Some(write) = state.write.as_mut() {
                if !state.write_submitted {
                    let result = match write {
                        Write::Stage { pending, .. } => pending.submit(saves),
                        Write::HandIn(pending) => pending.submit(saves),
                        Write::Checkpoint { pending, .. } => pending.submit(saves),
                    };
                    match result {
                        Ok(()) => {
                            state.write_submitted = true;
                            state.submission_failure = None;
                        }
                        Err(error) => {
                            let message = error.to_string();
                            if state.submission_failure.as_ref() != Some(&message) {
                                state.submission_failure = Some(message.clone());
                                self.events
                                    .push_back(NpcCoordinatorEvent::Held { source, message });
                            }
                            continue;
                        }
                    }
                }
                let result = match write {
                    Write::Checkpoint { pending, operation } => pending.poll().map(|resolution| {
                        use crate::npc_persistence::NpcCheckpointResolution as R;
                        let (action, terminal, committed) = match &resolution {
                            R::Committed => (
                                Some(A::CommitIdle {
                                    source,
                                    operation: *operation,
                                }),
                                true,
                                true,
                            ),
                            R::Rejected(_) => (
                                Some(A::ReleaseIdle {
                                    source,
                                    operation: *operation,
                                }),
                                true,
                                false,
                            ),
                            R::Uncertain(_) => (None, false, false),
                        };
                        (
                            action,
                            terminal,
                            committed,
                            NpcCoordinatorEvent::Checkpoint { source, resolution },
                        )
                    }),
                    Write::Stage { pending, adoption } => pending.poll().map(|resolution| {
                        let (action, terminal, committed) = match &resolution {
                            NpcStageResolution::Committed {
                                proposal,
                                adoption: kind,
                                ..
                            } => (Some(adoption.commit(proposal.clone(), *kind)), true, true),
                            NpcStageResolution::Rejected { proposal, .. } => {
                                (adoption.reject(proposal), true, false)
                            }
                            NpcStageResolution::Uncertain { .. } => (None, false, false),
                        };
                        (
                            action,
                            terminal,
                            committed,
                            NpcCoordinatorEvent::Durable {
                                source,
                                resolution: Box::new(resolution),
                            },
                        )
                    }),
                    Write::HandIn(pending) => pending.poll().map(|resolution| {
                        let (action, terminal, committed) = match &resolution {
                            NpcHandInResolution::Committed {
                                ticket, receipt, ..
                            } => (
                                Some(A::CommitHandIn {
                                    ticket: ticket.clone(),
                                    receipt: receipt.clone(),
                                }),
                                true,
                                true,
                            ),
                            NpcHandInResolution::Rejected { ticket, .. } => (
                                Some(A::RejectHandIn {
                                    ticket: ticket.clone(),
                                }),
                                true,
                                false,
                            ),
                            NpcHandInResolution::Uncertain(_) => (None, false, false),
                        };
                        (
                            action,
                            terminal,
                            committed,
                            NpcCoordinatorEvent::HandIn {
                                source,
                                resolution: Box::new(resolution),
                            },
                        )
                    }),
                };
                if let Some((action, terminal, committed, event)) = result {
                    state.write_submitted = false;
                    if committed {
                        let operation = match write {
                            Write::Checkpoint { pending, .. } => pending.operation(),
                            Write::Stage { pending, .. } => pending.operation(),
                            Write::HandIn(pending) => pending.operation(),
                        };
                        state.completed =
                            NpcWorkflowSaveV3::decode_or_migrate(&operation.workflow.checkpoint)
                                .expect("validated immutable NPC journal bytes")
                                .completed;
                        state.binding.source_version += 1;
                        state.workflow_version += 1;
                    }
                    if terminal {
                        state.write = None;
                    }
                    if let Some(action) = action {
                        // Correlation exhaustion is preflighted on stage admission.
                        let correlation = state.write_correlation;
                        state.delivery = Some(Delivery {
                            command: NpcServiceCommand {
                                correlation,
                                action,
                            },
                            submitted: false,
                            retry_on_failure: true,
                        });
                    }
                    self.events.push_back(event);
                }
            }
            if let Some(delivery) = state.delivery.as_mut()
                && !delivery.submitted
                && input
                    .try_submit(Command::NpcService(Box::new(delivery.command.clone())))
                    .is_ok()
            {
                delivery.submitted = true;
            }
        }
    }
    /// Unmatched or pressure-blocked responses remain with the caller. A failed
    /// postcommit adoption retains the exact command; it cannot rerun the write.
    pub fn accept(
        &mut self,
        outcome: Arc<NpcServiceOutcome>,
    ) -> Result<(), Arc<NpcServiceOutcome>> {
        if self.events.len() == self.capacity {
            return Err(outcome);
        }
        let Some((&source, state)) = self.sources.iter_mut().find(|(_, s)| {
            s.delivery
                .as_ref()
                .is_some_and(|d| d.submitted && d.command.correlation == outcome.correlation)
        }) else {
            return Err(outcome);
        };
        let delivery = state.delivery.as_mut().expect("matched delivery");
        if outcome.result.is_err() && delivery.retry_on_failure {
            delivery.submitted = false;
        } else {
            state.delivery = None;
            if state.recovering && state.recovery.is_empty() {
                state.recovering = false;
                state.retained_bytes = 0;
            }
        }
        self.events
            .push_back(NpcCoordinatorEvent::Command { source, outcome });
        Ok(())
    }
    pub fn take_event(&mut self) -> Option<NpcCoordinatorEvent> {
        self.events.pop_front()
    }
    /// Completed sources can release their mapped generation after the owner
    /// has drained notifications and explicitly released any detached archive.
    pub fn release_source(
        &mut self,
        source: EntityId,
    ) -> Result<(NpcCheckpointBinding, i64), String> {
        if self.busy(source)
            || self.events.iter().any(|e| match e {
                NpcCoordinatorEvent::Checkpoint { source: s, .. }
                | NpcCoordinatorEvent::Command { source: s, .. }
                | NpcCoordinatorEvent::Durable { source: s, .. }
                | NpcCoordinatorEvent::HandIn { source: s, .. }
                | NpcCoordinatorEvent::Held { source: s, .. } => *s == source,
            })
        {
            return Err("NPC source still has retained work".into());
        }
        let state = self.sources.remove(&source).ok_or("NPC source not bound")?;
        Ok((state.binding, state.workflow_version))
    }
}
fn stage_matches(state: &Source, operation: &bace_persistence::NpcStageOperation) -> bool {
    let Ok(checkpoint) = NpcWorkflowSaveV3::decode(&operation.workflow.checkpoint) else {
        return false;
    };
    checkpoint.source == state.binding.source
        && checkpoint.source_template == state.binding.source_template
        && checkpoint.program_hash == state.binding.program_hash
        && checkpoint.content_generation == state.binding.content_generation
        && checkpoint.invocation == state.binding.invocation
        && checkpoint.source_version == state.binding.source_version.saturating_add(1)
        && operation.workflow.expected_version == state.workflow_version
        && state.workflow_version < i64::MAX
        && state.binding.source_version < i64::MAX as u64
}
