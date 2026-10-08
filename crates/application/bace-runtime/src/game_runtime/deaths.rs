//! Retained death lifecycle; corpse source, valuable checkpoint and presentation
//! are separate obligations and none can be dropped when another lane blocks.
mod access;
mod access_saves;
mod cold;
mod consent;
mod expiry;
mod output;
mod sources;
use super::*;
use crate::player_death_service::{PlayerDeathCompletion, PlayerDeathService, PlayerDeathWork};
use bace_gameplay_api::CharacterBinding;
use bace_simulation::{
    Command, PlayerDeathCommand, PlayerDeathEvent, PlayerDeathServiceCommand,
    PlayerDeathServiceOutcome, PlayerDeathTicket, PlayerReadSnapshot, PlayerSnapshotOperation,
    PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
use bace_types::EntityId;
use std::sync::mpsc::TrySendError;
const LIMIT: usize = 64;

pub struct DeathDelivery {
    pub sequence: u64,
    pub work: DeathDeliveryWork,
}
pub enum DeathDeliveryWork {
    Expired {
        event: bace_simulation::CorpseExpiryEvent,
        ticket: bace_simulation::CorpseExpiryTicket,
        spill: Option<Arc<CorpseSpillPresentation>>,
    },
    Event(PlayerDeathEvent),
    /// Full source rows and immutable pre-death view are retained for canonical
    /// private messages, inventory changes and accepted corpse visibility.
    Completed {
        completion: Box<PlayerDeathCompletion>,
        before: Arc<PlayerReadSnapshot>,
        appearance: crate::player_entry::PreparedEntryAppearanceAssets,
        corpse_visibility: Option<crate::visibility_assets::PreparedVisibilityObject>,
        world_visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
    },
}
pub(super) struct CompletedPresentation {
    actor: EntityId,
    corpse: EntityId,
    binding: CharacterBinding,
    corpse_visibility: Option<crate::visibility_assets::PreparedVisibilityObject>,
    world_visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
}
pub struct CorpseSpillPresentation {
    pub snapshots: Vec<bace_persistence::SaveSnapshot>,
    pub visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
}
struct Request {
    operation: u64,
    actor: EntityId,
    before: u64,
    killer: Option<(EntityId, String)>,
    killer_is_olthoi: bool,
    olthoi: Option<bace_simulation::OlthoiDeathKind>,
}
enum Phase {
    Barrier,
    Capture,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>, u64),
    Cold,
    Ready,
    Submitted(u64),
    Proposal,
    Saving,
}
struct Pending {
    request: Request,
    key: SessionKey,
    binding: CharacterBinding,
    phase: Phase,
    proof: Option<Arc<PlayerReadSnapshot>>,
    cold: Option<cold::Metadata>,
    command: Option<PlayerDeathCommand>,
    ticket: Option<PlayerDeathTicket>,
    ids: Vec<EntityId>,
    loot_clock: Option<i32>,
    regions: std::collections::BTreeSet<u16>,
}
pub(super) struct DeathRuntime {
    access: Option<access::Pending>,
    viewers: BTreeMap<SessionKey, (EntityId, CharacterBinding)>,
    unexpected_access: Option<bace_simulation::CorpseAccessOutcome>,
    consent: Option<consent::Pending>,
    unexpected_consent: Option<bace_simulation::CorpseConsentOutcome>,
    expiry: expiry::Expiry,
    unmatched_expiry: Option<bace_simulation::CorpseExpiryEvent>,
    pub service: PlayerDeathService,
    requests: VecDeque<Request>,
    pending: Option<Pending>,
    job: Option<Job<cold::Completion>>,
    orphan_cold: Option<cold::Completion>,
    orphan_completion: Option<Box<PlayerDeathCompletion>>,
    unexpected: Option<Box<PlayerDeathServiceOutcome>>,
    unexpected_ticket: Option<PlayerDeathTicket>,
    deliveries: VecDeque<DeathDelivery>,
    completed_presentations: BTreeMap<u64, CompletedPresentation>,
    next: u64,
    blocked: Option<String>,
}
impl DeathRuntime {
    pub fn new() -> Self {
        Self {
            access: None,
            viewers: BTreeMap::new(),
            unexpected_access: None,
            consent: None,
            unexpected_consent: None,
            service: PlayerDeathService::new(),
            expiry: expiry::Expiry::new(),
            unmatched_expiry: None,
            requests: VecDeque::new(),
            pending: None,
            job: None,
            orphan_cold: None,
            orphan_completion: None,
            unexpected: None,
            unexpected_ticket: None,
            deliveries: VecDeque::new(),
            completed_presentations: BTreeMap::new(),
            next: 0,
            blocked: None,
        }
    }
    pub fn has_pending(&self) -> bool {
        self.access.is_some()
            || !self.viewers.is_empty()
            || self.unexpected_access.is_some()
            || self.consent.is_some()
            || self.unexpected_consent.is_some()
            || self.expiry.has_pending()
            || self.unmatched_expiry.is_some()
            || self.orphan_cold.is_some()
            || self.orphan_completion.is_some()
            || self.service.requires_drain()
            || self.pending.is_some()
            || self.job.is_some()
            || !self.requests.is_empty()
            || !self.deliveries.is_empty()
            || !self.completed_presentations.is_empty()
            || self.unexpected.is_some()
            || self.unexpected_ticket.is_some()
    }
    pub fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.service.owns_capture(outcome)
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.phase,Phase::Capturing(id) if id==outcome.correlation))
    }
    pub fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if self.service.owns_capture(&outcome) {
            return self.service.accept_capture(outcome, unix);
        }
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| matches!(p.phase,Phase::Capturing(id) if id==outcome.correlation))
        else {
            return Err(outcome);
        };
        match &outcome.result {
            Ok(s)
                if s.binding() == p.binding
                    && s.operation()
                        == Some((
                            PlayerSnapshotOperation::PlayerDeath(p.request.operation),
                            p.request.before,
                        )) =>
            {
                p.phase = Phase::Captured(s.clone(), unix)
            }
            Err(e) => {
                self.blocked = Some(format!("death preparation capture: {e:?}"));
                p.phase = Phase::Capture;
            }
            _ => return Err(outcome),
        }
        Ok(())
    }
    fn room(&self) -> bool {
        self.deliveries.len() < LIMIT && self.next < u64::MAX
    }
    fn push(&mut self, work: DeathDeliveryWork) {
        self.next += 1;
        self.deliveries.push_back(DeathDelivery {
            sequence: self.next,
            work,
        });
    }
    fn push_completed(&mut self, work: DeathDeliveryWork, operation: u64) {
        self.next += 1;
        let index = self
            .deliveries
            .iter()
            .position(|delivery| match &delivery.work {
                DeathDeliveryWork::Event(PlayerDeathEvent::Corpse {
                    operation: event, ..
                })
                | DeathDeliveryWork::Event(PlayerDeathEvent::WorldDrops {
                    operation: event, ..
                }) => *event == operation,
                _ => false,
            })
            .unwrap_or(self.deliveries.len());
        self.deliveries.insert(
            index,
            DeathDelivery {
                sequence: self.next,
                work,
            },
        );
    }
}
impl GameRuntime {
    pub fn pending_death_delivery(&self) -> Option<&DeathDelivery> {
        self.deaths.deliveries.front()
    }
    pub fn acknowledge_death_delivery(&mut self, sequence: u64) -> Result<(), String> {
        if self
            .deaths
            .deliveries
            .front()
            .is_none_or(|v| v.sequence != sequence)
        {
            return Err("death presentation acknowledgment mismatch".into());
        }
        self.deaths.deliveries.pop_front();
        Ok(())
    }
    pub fn death_failure(&self) -> Option<&str> {
        self.deaths
            .blocked
            .as_deref()
            .or(self.deaths.service.blocked())
    }
    pub(super) fn death_session_pending(&self, key: SessionKey) -> bool {
        self.deaths.access.as_ref().is_some_and(|p| p.key == key)
            || self.deaths.consent.as_ref().is_some_and(|p| p.key == key)
            || self.deaths.pending.as_ref().is_some_and(|p| p.key == key)
            || self
                .sessions
                .get(&key)
                .and_then(|s| s.loading.as_ref())
                .is_some_and(|l| {
                    self.deaths
                        .requests
                        .iter()
                        .any(|r| r.actor == l.loaded.binding.actor)
                })
    }
    pub(super) fn retry_death_session(&mut self, key: SessionKey) -> bool {
        if !self.death_session_pending(key) {
            return false;
        }
        if self.deaths.service.blocked().is_some() {
            return self.deaths.service.retry().unwrap_or(false);
        }
        if self.deaths.blocked.take().is_some() {
            if let Some(p) = &mut self.deaths.pending
                && matches!(p.phase, Phase::Ready | Phase::Cold)
            {
                p.command = None;
                p.cold = None;
                p.phase = Phase::Capture;
            }
            true
        } else {
            false
        }
    }
    pub(super) fn poll_deaths(&mut self, elapsed: Duration, unix: u64) -> Result<(), String> {
        self.poll_corpse_consent()?;
        self.poll_corpse_access(unix)?;
        self.register_death_portal_watches()?;
        for _ in 0..self.limits.work_per_poll {
            if !self.deaths.room() || self.deaths.requests.len() >= LIMIT {
                break;
            }
            let Ok(event) = self.simulation.player_death_events().try_recv() else {
                break;
            };
            if let PlayerDeathEvent::Prepare {
                operation,
                actor,
                before_revision,
                corpse_killer,
                killer_is_olthoi,
                olthoi,
                ..
            } = &event
            {
                self.deaths.requests.push_back(Request {
                    operation: *operation,
                    actor: *actor,
                    before: *before_revision,
                    killer: corpse_killer.clone(),
                    killer_is_olthoi: *killer_is_olthoi,
                    olthoi: *olthoi,
                });
            }
            self.deaths.service.observe_event(&event);
            self.deaths.push(DeathDeliveryWork::Event(event));
            self.register_death_portal_watches()?;
        }
        if self.deaths.orphan_cold.is_some()
            || self.deaths.unexpected.is_some()
            || self.deaths.unexpected_ticket.is_some()
        {
            return Err("unmatched death output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.player_death_service_outcomes().try_recv() else {
                break;
            };
            if self.deaths.expiry.owns(&outcome) {
                self.deaths.expiry.accept(outcome);
                continue;
            }
            if self.deaths.service.owns_outcome(&outcome) {
                self.deaths.service.accept_outcome(outcome).map_err(|o| {
                    self.deaths.unexpected = Some(o);
                    "death receipt correlation".to_string()
                })?;
                continue;
            }
            let Some(p) = self
                .deaths
                .pending
                .as_mut()
                .filter(|p| matches!(p.phase,Phase::Submitted(id) if id==outcome.correlation))
            else {
                self.deaths.unexpected = Some(Box::new(outcome));
                return Err("unmatched death preparation outcome".into());
            };
            match outcome.result {
                Ok(()) => p.phase = Phase::Proposal,
                Err((error, command)) => {
                    p.command = Some(*command);
                    p.phase = Phase::Ready;
                    self.deaths.blocked = Some(format!("death preparation rejected: {error:?}"));
                }
            }
        }
        if self.deaths.unexpected_ticket.is_none()
            && let Ok(ticket) = self.simulation.player_death_proposals().try_recv()
        {
            if let Some(p) = self.deaths.pending.as_mut().filter(|p| {
                p.request.operation == ticket.operation
                    && p.binding.actor == ticket.actor
                    && p.ticket.is_none()
            }) {
                p.ticket = Some(ticket);
            } else {
                self.deaths.unexpected_ticket = Some(ticket);
                return Err("unmatched death proposal retained".into());
            }
        }
        if let Some(done) = ready(&mut self.deaths.job) {
            let Some(p) = self.deaths.pending.as_mut() else {
                self.deaths.orphan_cold = Some(done);
                return Err("death cold owner missing".into());
            };
            p.ids = done.identities;
            match done.result {
                Ok(value) => {
                    // Keep immutable source/appearance alongside the sole prepared
                    // Actor until its trusted command transfers ownership.
                    p.command = Some(value.command);
                    // The remaining cold metadata is stored independently below.
                    p.cold = Some(value.metadata);
                    p.phase = Phase::Ready;
                }
                Err(error) => {
                    self.deaths.blocked = Some(error);
                    p.phase = Phase::Cold;
                }
            }
        }
        if self.deaths.room() {
            if self.deaths.orphan_completion.is_none() {
                self.deaths.orphan_completion = self.deaths.service.take_completion().map(Box::new);
            }
            if let Some(completion) = self.deaths.orphan_completion.as_ref() {
                if self
                    .deaths
                    .pending
                    .as_ref()
                    .is_none_or(|p| p.proof.is_none() || p.cold.is_none())
                {
                    return Err("death completion source owner missing".into());
                }
                let Some(world) = self.world.as_mut() else {
                    return Err("death completed region owner missing".into());
                };
                if let Some(plan) = &completion.work.ticket.no_corpse {
                    let sources = sources::committed_world_drops(completion)?;
                    world.regions.record_committed_world_item_sources(
                        (plan.accepted_position.obj_cell_id >> 16) as u16,
                        sources,
                    )?;
                } else {
                    let sources = sources::committed(completion)?;
                    let corpse = sources
                        .first()
                        .and_then(|source| source.corpse.as_ref())
                        .ok_or("death completed corpse source missing")?;
                    let landblock = match &corpse.placement {
                        bace_storage_codec::ItemPlacementV2::World(p) => {
                            (p.obj_cell_id >> 16) as u16
                        }
                        _ => return Err("death completed corpse placement invalid".into()),
                    };
                    world
                        .regions
                        .record_committed_corpse_sources(landblock, sources)?;
                }
                let completion = *self
                    .deaths
                    .orphan_completion
                    .take()
                    .expect("retained completion");
                let p = self.deaths.pending.take().expect("validated death owner");
                let before = p.proof.expect("validated death capture");
                let metadata = p.cold.expect("validated death assets");
                let operation = completion.work.ticket.operation;
                self.deaths.push_completed(
                    DeathDeliveryWork::Completed {
                        completion: Box::new(completion),
                        before,
                        appearance: metadata.appearance,
                        corpse_visibility: metadata.corpse_visibility,
                        world_visibility: metadata.world_visibility,
                    },
                    operation,
                );
            }
        }
        if let Some(error) = &self.deaths.blocked {
            return Err(error.clone());
        }
        if self.deaths.pending.is_none()
            && self.deaths.room()
            && let Some(request) = self.deaths.requests.front()
        {
            let (key, binding) = self
                .sessions
                .iter()
                .filter_map(|(key, s)| s.loading.as_ref().map(|l| (*key, l)))
                .find(|(_, l)| l.loaded.binding.actor == request.actor)
                .map(|(key, l)| (key, l.loaded.binding))
                .ok_or("dead player session missing")?;
            let request = self
                .deaths
                .requests
                .pop_front()
                .expect("checked death request");
            self.deaths.pending = Some(Pending {
                request,
                key,
                binding,
                phase: Phase::Barrier,
                proof: None,
                cold: None,
                command: None,
                ticket: None,
                ids: vec![],
                loot_clock: None,
                regions: Default::default(),
            });
        }
        self.advance_death_preparation(unix)?;
        let correlation = self.token()?;
        let tick = u64::try_from(elapsed.as_nanos() * 30 / 1_000_000_000)
            .map_err(|_| "death tick overflow")?;
        let input = self.simulation.input();
        self.deaths.service.poll_with(
            correlation,
            unix,
            tick,
            &mut self.online_saves,
            &self.saves.handle,
            |c| input.try_submit(c).map_err(Box::new),
        )?;
        self.project_death_deliveries()
    }
}
