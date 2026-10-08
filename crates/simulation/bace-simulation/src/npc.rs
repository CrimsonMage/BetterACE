//! Native NPC orchestration on the simulation owner. Owner proposals remain
//! reserved until exact durable adoption; packet/service work is bounded output.
mod admission;
mod archive;
pub use admission::{NpcScriptIdentity, PreparedNpcScriptSource};
mod handin;
mod host;
mod idle;
mod inventory_snapshot;
pub use archive::{NpcSourceArchive, NpcSourceLocation};
pub use inventory_snapshot::NpcSourceInventorySnapshot;
mod signals;
pub use handin::{NpcHandInRequest, NpcHandInTicket};
mod queued_experience;
mod services;
pub(crate) use services::NpcServiceView;
pub use services::{NpcInvocationCheckpoint, NpcPendingCheckpoint, NpcSourceCheckpoint};
mod text;
use crate::characters::Characters;
use bace_character::{ExperienceCredit, LuminanceCredit};
use bace_emotes::{NativeEmoteManager, NativeError, NativeProgram, NativeTrigger};
use bace_entity::PropertyChange;
use bace_gameplay_api::{NpcCompletion, NpcContext, NpcFailure, NpcOperation};
use bace_quests::{QuestChange, QuestDefinition, QuestRegistry};
use bace_random::{Domain, RandomRoot, RandomStream};
use bace_types::EntityId;
use bace_world::World;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcAggregateFence {
    pub before_revision: u64,
    pub after_revision: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum NpcEffect {
    QueuedExperience {
        actor: EntityId,
        amount: u64,
        share: NpcExperienceSharing,
        phase: NpcQueuedExperiencePhase,
    },
    EarnedExperience {
        actor: EntityId,
        change: bace_character::EarnedExperienceChange,
    },
    CharacterService {
        actor: EntityId,
        change: bace_character::CharacterServiceChange,
    },
    Contract {
        actor: EntityId,
        before_revision: u64,
        change: bace_quests::ContractChange,
    },
    Property {
        actor: EntityId,
        aggregate: Option<NpcAggregateFence>,
        change: PropertyChange,
    },
    Quest {
        actor: EntityId,
        aggregate: Option<NpcAggregateFence>,
        change: QuestChange,
    },
    FellowQuest {
        fellowship: u64,
        change: QuestChange,
    },
    Experience {
        actor: EntityId,
        credit: ExperienceCredit,
    },
    Luminance {
        actor: EntityId,
        credit: LuminanceCredit,
    },
    /// Named owner must implement/commit this request before returning its receipt.
    Service(NpcOperation),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcExperienceSharing {
    None,
    Allegiance,
    All,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcQueuedExperiencePhase {
    AwaitingAdmission,
    Ready,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcProposal {
    pub ticket: u64,
    pub context: NpcContext,
    pub effect: NpcEffect,
}
/// Exact inventory proposal bound to one authored NPC row. Only the joint
/// inventory/workflow receipt may release this reservation.
#[derive(Clone, Debug, PartialEq)]
pub struct NpcInventoryTicket {
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub character_revision: u64,
    pub npc: NpcProposal,
    pub inventory: crate::InventoryTicket,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcSpellbookTicket {
    pub npc: NpcProposal,
    pub actor: EntityId,
    pub spell: u32,
    pub before_revision: u64,
    pub after_revision: u64,
    pub before: Vec<u32>,
    pub after: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcPortalTicket {
    pub npc: NpcProposal,
    pub portal: crate::PortalServiceTicket,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcDeleteSourceTicket {
    pub npc: NpcProposal,
    pub archive: NpcSourceArchive,
    pub hold: bace_world::WorldRetirementHold,
    pub origin: Option<crate::GeneratedNpcOrigin>,
    pub retirement: Option<crate::GeneratedRetirementTicket>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcTrainingCreditTicket {
    pub npc: NpcProposal,
    pub actor: EntityId,
    pub change: bace_character::TrainingCreditChange,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcSkillResetTicket {
    pub npc: NpcProposal,
    pub actor: EntityId,
    pub change: bace_character::SkillTransitionChange,
    pub before_revision: u64,
}
pub(crate) struct NpcMoveState {
    pub proposal: NpcProposal,
    pub actor: EntityId,
    pub epoch: u16,
    pub control: bace_motion::TurnControl,
    pub destination: bace_gameplay_api::NpcDestination,
    pub turn_only: bool,
    pub speed: f32,
    pub deadline: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcNotification {
    pub context: NpcContext,
    pub operation: NpcOperation,
}
struct Pending {
    proposal: NpcProposal,
    completion: NpcCompletion,
    adopted: bool,
    detached: bool,
}
struct Source {
    admission_bound: bool,
    admission_hold: Option<bace_world::WorldNpcAdmissionHold>,
    admission: Option<NpcScriptIdentity>,
    manager: NativeEmoteManager,
    random: Option<RandomStream>,
    use_radius: f32,
    clock_offset: f64,
    event_id: [u8; 16],
    key_version: u32,
    active_operation: u64,
    invocations: BTreeMap<u64, NpcInvocationCheckpoint>,
    recovery_ready: bool,
    journal_hold: Option<(u64, f64)>,
    held_logical_now: Option<f64>,
}
pub(crate) struct NativeInvocation {
    pub actor: EntityId,
    pub target: Option<EntityId>,
    pub trigger: NativeTrigger,
    pub event: [u8; 16],
    pub operation: u64,
    pub check_range: bool,
}
pub(crate) struct Npcs {
    durable_dirty: std::collections::BTreeSet<EntityId>,
    pub(crate) source_inventory: BTreeMap<EntityId, NpcSourceInventorySnapshot>,
    pub(crate) deletion_retired: std::collections::BTreeSet<u64>,
    pub(crate) deletion_committed: std::collections::BTreeSet<u64>,
    pub(crate) deletion_registries: std::collections::BTreeSet<u64>,
    pub(crate) deletions: BTreeMap<u64, NpcDeleteSourceTicket>,
    pub(crate) archives: BTreeMap<EntityId, NpcSourceArchive>,
    pub(crate) damage_events: VecDeque<crate::CombatEvent>,
    pub(crate) portal_services: BTreeMap<u64, NpcPortalTicket>,
    pub(crate) skill_resets:
        BTreeMap<u64, (NpcSkillResetTicket, Option<bace_character::SkillProposal>)>,
    source_order: Vec<EntityId>,
    pub(crate) handins: BTreeMap<u64, (NpcHandInTicket, bool)>,
    pub(crate) training_credit_services: BTreeMap<u64, NpcTrainingCreditTicket>,
    pub(crate) spellbook_services: BTreeMap<u64, NpcSpellbookTicket>,
    pub(crate) spellbook_registries: std::collections::BTreeSet<u64>,
    pub(crate) spell_catalog: Option<std::collections::BTreeSet<u32>>,
    pub(crate) moves: BTreeMap<u64, NpcMoveState>,
    pub(crate) inventory_services: BTreeMap<u64, NpcInventoryTicket>,
    pub(crate) cast_services: BTreeMap<u64, NpcProposal>,
    pub(crate) cast_completions: BTreeMap<u64, bace_gameplay_api::ServerCastOutcome>,
    pub(crate) motion_services:
        BTreeMap<u64, (NpcProposal, EntityId, u16, bace_motion::MotionToken)>,
    pub(crate) motion_completions: BTreeMap<u64, bace_world::WorldMotionEvent>,
    pub(crate) shared_experience: bool,
    sources: BTreeMap<EntityId, Source>,
    quests: BTreeMap<EntityId, QuestRegistry>,
    definitions: BTreeMap<String, QuestDefinition>,
    level_table: Option<Arc<bace_character::CharacterLevelTable>>,
    xp_rates: Option<(f64, f64)>,
    contract_catalog_loaded: bool,
    contract_definitions: BTreeMap<u32, String>,
    events: bace_world_events::Events,
    pending: BTreeMap<u64, Pending>,
    proposals: VecDeque<NpcProposal>,
    notifications: VecDeque<NpcNotification>,
    root: Option<Arc<RandomRoot>>,
    epoch: Option<u32>,
    capacity: usize,
    next_ticket: u64,
    ticket_limit: u64,
    scratch: Vec<EntityId>,
    resumed: Vec<u64>,
}
impl Npcs {
    pub(crate) fn generator_event(
        &mut self,
        name: &str,
        now: i32,
    ) -> Result<bace_gameplay_api::GeneratorEventState, bace_world_events::EventError> {
        let Some(state) = self.events.state(name) else {
            return Ok(bace_gameplay_api::GeneratorEventState::Missing);
        };
        let enabled = state != bace_world_events::EventState::Disabled;
        let started = self.events.started(name, now)?;
        Ok(bace_gameplay_api::GeneratorEventState::Available { enabled, started })
    }

    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.min(4096);
        Self {
            durable_dirty: Default::default(),
            source_inventory: BTreeMap::new(),
            deletion_retired: std::collections::BTreeSet::new(),
            deletion_committed: std::collections::BTreeSet::new(),
            deletion_registries: std::collections::BTreeSet::new(),
            deletions: BTreeMap::new(),
            archives: BTreeMap::new(),
            damage_events: VecDeque::with_capacity(capacity),
            portal_services: BTreeMap::new(),
            skill_resets: BTreeMap::new(),
            source_order: Vec::with_capacity(capacity),
            handins: BTreeMap::new(),
            training_credit_services: BTreeMap::new(),
            spellbook_services: BTreeMap::new(),
            spellbook_registries: std::collections::BTreeSet::new(),
            spell_catalog: None,
            moves: BTreeMap::new(),
            inventory_services: BTreeMap::new(),
            cast_services: BTreeMap::new(),
            cast_completions: BTreeMap::new(),
            motion_services: BTreeMap::new(),
            motion_completions: BTreeMap::new(),
            shared_experience: false,
            sources: BTreeMap::new(),
            quests: BTreeMap::new(),
            definitions: BTreeMap::new(),
            level_table: None,
            xp_rates: None,
            contract_catalog_loaded: false,
            contract_definitions: BTreeMap::new(),
            events: bace_world_events::Events::prepare(vec![], false)
                .expect("empty event registry"),
            pending: BTreeMap::new(),
            proposals: VecDeque::with_capacity(capacity),
            notifications: VecDeque::with_capacity(capacity),
            root: None,
            epoch: None,
            capacity,
            next_ticket: 0,
            ticket_limit: u64::MAX,
            scratch: Vec::with_capacity(capacity),
            resumed: Vec::with_capacity(capacity),
        }
    }
    pub(crate) fn configure(
        &mut self,
        root: Arc<RandomRoot>,
        epoch: u32,
        definitions: Vec<(String, QuestDefinition)>,
        events: bace_world_events::Events,
    ) -> Result<(), NpcFailure> {
        if self.has_active() || definitions.len() > 65536 {
            return Err(NpcFailure::Capacity);
        }
        let mut prepared = BTreeMap::new();
        for (name, definition) in definitions {
            let key = bace_quests::quest_key(&name).map_err(|_| NpcFailure::InvalidInput)?;
            if prepared.insert(key, definition).is_some() {
                return Err(NpcFailure::InvalidInput);
            }
        }
        self.root = Some(root);
        self.epoch = Some(epoch);
        self.definitions = prepared;
        self.events = events;
        Ok(())
    }
    pub(crate) fn register(
        &mut self,
        actor: EntityId,
        program: Arc<NativeProgram>,
        use_radius: f32,
        world: &World,
    ) -> Result<(), NpcFailure> {
        if world.body(actor).is_err() {
            return Err(NpcFailure::MissingActor);
        }
        if !use_radius.is_finite() {
            return Err(NpcFailure::InvalidInput);
        }
        if self.sources.contains_key(&actor) {
            return Err(NpcFailure::Conflict);
        }
        if self.sources.len() >= self.capacity
            || !self.quests.contains_key(&actor) && self.quests.len() >= 4096
        {
            return Err(NpcFailure::Capacity);
        }
        self.sources.insert(
            actor,
            Source {
                admission_bound: false,
                admission_hold: None,
                admission: None,
                manager: NativeEmoteManager::new(program),
                random: None,
                use_radius,
                clock_offset: 0.0,
                event_id: [0; 16],
                key_version: 0,
                active_operation: 0,
                invocations: BTreeMap::new(),
                recovery_ready: true,
                journal_hold: None,
                held_logical_now: None,
            },
        );
        self.quests
            .entry(actor)
            .or_insert(QuestRegistry::new(4096).map_err(|_| NpcFailure::Capacity)?);
        self.source_order.push(actor);
        Ok(())
    }
    pub(crate) fn register_quests(
        &mut self,
        actor: EntityId,
        quests: QuestRegistry,
    ) -> Result<(), NpcFailure> {
        if self.quests.contains_key(&actor) {
            return Err(NpcFailure::Conflict);
        }
        if self.quests.len() >= 4096 {
            return Err(NpcFailure::Capacity);
        }
        self.quests.insert(actor, quests);
        Ok(())
    }
    pub(crate) fn has_active(&self) -> bool {
        self.sources.values().any(|s| s.manager.busy()) || !self.pending.is_empty()
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.sources.is_empty()
            || !self.pending.is_empty()
            || !self.notifications.is_empty()
            || !self.proposals.is_empty()
    }
    pub(crate) fn reserved(&self, actor: EntityId) -> bool {
        self.sources
            .get(&actor)
            .is_some_and(|s| s.journal_hold.is_some())
            || self
                .handins
                .values()
                .any(|(h, _)| h.request.context.actor == actor || h.request.source == actor)
            || self.pending.values().any(|p| {
                !matches!(p.proposal.effect, NpcEffect::QueuedExperience { .. })
                    && (p.proposal.context.target == Some(actor)
                        || p.proposal.context.source == actor)
            })
    }
    pub(crate) fn has_source(&self, actor: EntityId) -> bool {
        self.sources.contains_key(&actor)
    }
    /// Queued source work does not lock ordinary owner mutations, but its
    /// participants must remain loaded until the durable obligation completes.
    pub(crate) fn pending_participant(&self, actor: EntityId) -> bool {
        if self.durable_dirty.contains(&actor) {
            return true;
        }
        if self.source_inventory.contains_key(&actor)
            || self
                .sources
                .get(&actor)
                .is_some_and(|s| s.journal_hold.is_some() || !s.recovery_ready)
        {
            return true;
        }
        if self
            .sources
            .values()
            .any(|s| s.manager.references_actor(actor))
        {
            return true;
        }
        self.handins.values().any(|(h,_)|h.request.context.actor==actor||h.request.source==actor)||self.pending.values().any(|p| {
            p.proposal.context.target == Some(actor)
                || p.proposal.context.source == actor
                || matches!(p.proposal.effect, NpcEffect::QueuedExperience { actor: recipient, .. } if recipient == actor)
        })
    }
    pub(crate) fn take_proposal(&mut self) -> Option<NpcProposal> {
        self.proposals.pop_front()
    }
    pub(crate) fn take_notification(&mut self) -> Option<NpcNotification> {
        self.notifications.pop_front()
    }
    pub(crate) fn quest(&self, actor: EntityId, name: &str) -> Option<bace_quests::QuestProgress> {
        self.quests.get(&actor).and_then(|q| q.get(name))
    }
    pub(crate) fn start(
        &mut self,
        invocation: NativeInvocation,
        world: &mut World,
        characters: &mut Characters,
        fellowships: &mut crate::fellowships::Fellowships,
        tick: u64,
        services: &dyn NpcServiceView,
    ) -> Result<bool, NativeError> {
        let NativeInvocation {
            actor,
            target,
            trigger,
            event,
            operation,
            check_range,
        } = invocation;
        if world.body(actor).is_err() {
            return Err(NativeError::Owner(NpcFailure::MissingActor));
        }
        if services.reserved(actor) {
            return Err(NativeError::Busy);
        }
        if self
            .handins
            .values()
            .any(|(h, _)| h.request.source == actor)
        {
            return Err(NativeError::Busy);
        }
        let mut source = self
            .sources
            .remove(&actor)
            .ok_or(NativeError::Owner(NpcFailure::MissingActor))?;
        let result = (|| {
            if check_range {
                let target = target.ok_or(NativeError::Owner(NpcFailure::MissingActor))?;
                if !signals::within_use_radius(world, target, actor, source.use_radius)? {
                    return Err(NativeError::Owner(NpcFailure::InvalidInput));
                }
            }
            if !source.recovery_ready || source.journal_hold.is_some() || source.manager.busy() {
                return Err(NativeError::Busy);
            }
            source.invocations.retain(|operation, _| {
                self.pending.values().any(|p| {
                    p.proposal.context.source == actor && p.proposal.context.operation == *operation
                })
            });
            if self.pending.values().any(|p| {
                p.proposal.context.source == actor
                    && p.proposal.context.operation == source.active_operation
            }) {
                source.invocations.insert(
                    source.active_operation,
                    NpcInvocationCheckpoint {
                        operation: source.active_operation,
                        event_id: source.event_id,
                        key_version: source.key_version,
                        random_position: source.random.as_ref().map_or(0, RandomStream::position),
                    },
                );
            }
            if source.invocations.len() >= 76 || source.invocations.contains_key(&operation) {
                return Err(NativeError::Capacity);
            }
            source.active_operation = operation;
            source.event_id = event;
            source.key_version = self
                .root
                .as_ref()
                .ok_or(NpcFailure::MissingContent)?
                .key_version();
            source.random = Some(
                self.root
                    .as_ref()
                    .ok_or(NpcFailure::MissingContent)?
                    .event_stream(event, Domain::Npc)
                    .map_err(|_| NpcFailure::InvalidInput)?,
            );
            let context = NpcContext {
                source: actor,
                target,
                operation,
            };
            let now = tick as f64 / 30.0 + source.clock_offset;
            let mut host = host::Host {
                state: self,
                world,
                characters,
                fellowships,
                random: source.random.as_mut().expect("prepared stream"),
                tick,
                services,
            };
            source.manager.trigger(&trigger, context, now, &mut host)
        })();
        self.sources.insert(actor, source);
        result
    }
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        characters: &mut Characters,
        fellowships: &mut crate::fellowships::Fellowships,
        tick: u64,
        services: &dyn NpcServiceView,
    ) -> Vec<(EntityId, NativeError)> {
        self.resumed.clear();
        self.resumed.extend(
            self.pending
                .iter()
                .filter(|(_, p)| {
                    p.adopted
                        && self
                            .sources
                            .get(&p.proposal.context.source)
                            .is_some_and(|s| s.recovery_ready && s.journal_hold.is_none())
                })
                .map(|(ticket, _)| *ticket),
        );
        let mut errors = Vec::new();
        for index in 0..self.resumed.len() {
            let proposal = self.pending[&self.resumed[index]].proposal.clone();
            if let Err(e) = self.confirm(&proposal, world, characters, fellowships, tick, services)
            {
                errors.push((proposal.context.source, NativeError::Owner(e)));
            }
        }
        self.scratch.clear();
        self.scratch.extend(self.sources.keys().copied());
        for index in 0..self.scratch.len() {
            let id = self.scratch[index];
            let mut source = self.sources.remove(&id).expect("owned source");
            if source.recovery_ready
                && source.journal_hold.is_none()
                && source.manager.busy()
                && let Some(random) = source.random.as_mut()
            {
                let mut host = host::Host {
                    state: self,
                    world,
                    characters,
                    fellowships,
                    random,
                    tick,
                    services,
                };
                for _ in 0..256 {
                    match source
                        .manager
                        .step(tick as f64 / 30.0 + source.clock_offset, &mut host)
                    {
                        Ok(bace_emotes::NativeStep::Waiting | bace_emotes::NativeStep::Idle) => {
                            break;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            errors.push((id, error));
                            break;
                        }
                    }
                }
            }
            self.sources.insert(id, source);
        }
        self.prepare_queued_experience(world, characters, services, &mut errors);
        errors
    }
    pub(crate) fn confirm(
        &mut self,
        receipt: &NpcProposal,
        world: &mut World,
        characters: &mut Characters,
        fellowships: &mut crate::fellowships::Fellowships,
        tick: u64,
        services: &dyn NpcServiceView,
    ) -> Result<(), NpcFailure> {
        let pending = self
            .pending
            .get(&receipt.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *receipt {
            return Err(NpcFailure::Conflict);
        }
        if self
            .sources
            .get(&receipt.context.source)
            .and_then(|s| s.journal_hold)
            .is_some_and(|(ticket, _)| ticket != receipt.ticket)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        let completion = pending.completion;
        if !pending.adopted {
            let aggregate = match &receipt.effect {
                NpcEffect::Property {
                    actor, aggregate, ..
                }
                | NpcEffect::Quest {
                    actor, aggregate, ..
                } => aggregate.map(|f| (*actor, f)),
                _ => None,
            };
            if let Some((actor, fence)) = aggregate {
                characters
                    .validate_npc_aggregate(actor, fence)
                    .map_err(|_| NpcFailure::Conflict)?;
            }
            match &receipt.effect {
                NpcEffect::EarnedExperience { actor, change } => characters
                    .adopt_earned_experience(*actor, change.clone())
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::CharacterService { actor, change } => characters
                    .adopt_native_services(*actor, change.clone())
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::Contract {
                    actor,
                    before_revision,
                    change,
                } => characters
                    .adopt_contract(*actor, *before_revision, change.clone())
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::Property { actor, change, .. } => {
                    let properties = if let Some(properties) = world.properties_mut(*actor) {
                        properties
                    } else {
                        &mut self
                            .archives
                            .get_mut(actor)
                            .ok_or(NpcFailure::MissingActor)?
                            .properties
                    };
                    properties
                        .adopt(change.clone())
                        .map_err(|_| NpcFailure::Conflict)?;
                }
                NpcEffect::Quest { actor, change, .. } => self
                    .quests
                    .get_mut(actor)
                    .ok_or(NpcFailure::MissingActor)?
                    .adopt(change.clone())
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::FellowQuest { fellowship, change } => fellowships
                    .quests
                    .get_mut(fellowship)
                    .ok_or(NpcFailure::MissingActor)?
                    .adopt(change.clone())
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::Experience { actor, credit } => characters
                    .adopt_script_experience(*actor, *credit)
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::Luminance { actor, credit } => characters
                    .adopt_script_luminance(*actor, *credit)
                    .map_err(|_| NpcFailure::Conflict)?,
                NpcEffect::Service(_) | NpcEffect::QueuedExperience { .. } => {
                    return Err(NpcFailure::Unsupported);
                }
            }
            if let Some((actor, fence)) = aggregate {
                characters
                    .adopt_npc_aggregate(actor, fence)
                    .map_err(|_| NpcFailure::Conflict)?;
            }
            self.pending
                .get_mut(&receipt.ticket)
                .expect("retained proposal")
                .adopted = true;
        }
        self.release_journal_hold(receipt, tick)?;
        self.complete(
            receipt.ticket,
            completion,
            world,
            characters,
            fellowships,
            tick,
            services,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit borrowed subsystem owners; no duplicated aggregate state"
    )]
    fn complete(
        &mut self,
        ticket: u64,
        completion: NpcCompletion,
        world: &mut World,
        characters: &mut Characters,
        fellowships: &mut crate::fellowships::Fellowships,
        tick: u64,
        services: &dyn NpcServiceView,
    ) -> Result<(), NpcFailure> {
        let source_id = self
            .pending
            .get(&ticket)
            .ok_or(NpcFailure::Conflict)?
            .proposal
            .context
            .source;
        let mut source = self
            .sources
            .remove(&source_id)
            .ok_or(NpcFailure::MissingActor)?;
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
            if host.state.pending.get(&ticket).is_some_and(|p| p.detached) {
                return source
                    .manager
                    .complete_detached(ticket)
                    .map_err(|_| NpcFailure::Conflict);
            }
            source
                .manager
                .complete(
                    ticket,
                    completion,
                    tick as f64 / 30.0 + source.clock_offset,
                    &mut host,
                )
                .map_err(|_| NpcFailure::Conflict)
        })();
        self.sources.insert(source_id, source);
        result?;
        self.pending.remove(&ticket);
        self.proposals.retain(|p| p.ticket != ticket);
        Ok(())
    }
}
