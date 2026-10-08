//! Retained portal durability and output handoff. Composite NPC/death operations
//! remain with their transaction owners and cannot enter the generic save lane.
mod controls;
mod disconnect;
mod output;
mod publications;
mod summons;
use super::*;
use crate::portal_service::{PortalCompletion, PortalService, PortalWork};
use bace_simulation::{PortalResolutionOutcome, PortalServiceEvent, PortalServiceTicket};
use bace_types::EntityId;

const DELIVERY_CAPACITY: usize = 64;
/// The output adapter must retain and acknowledge the exact sequence only after
/// canonical private output and accepted visibility routing own its effects.
pub struct PortalDelivery {
    pub sequence: u64,
    pub work: PortalDeliveryWork,
    next_actor: usize,
}
pub enum PortalDeliveryWork {
    Event(PortalServiceEvent),
    Completed(Box<PortalCompletion>),
}
type PreparedSummonResult = (
    u64,
    EntityId,
    u32,
    Result<crate::visibility_assets::PreparedVisibilityObject, String>,
);
type ReadySummon = (
    u64,
    EntityId,
    u32,
    crate::visibility_assets::PreparedVisibilityObject,
);
pub(super) struct PortalRuntime {
    pub(super) service: PortalService,
    ticket: Option<PortalServiceTicket>,
    unmatched_resolution: Option<PortalResolutionOutcome>,
    deliveries: VecDeque<PortalDelivery>,
    tickets: BTreeMap<u64, PortalServiceTicket>,
    blocked_effects: std::collections::BTreeSet<u64>,
    next: u64,
    publications: publications::PortalPublications,
    transits: BTreeMap<EntityId, controls::Transit>,
    death_materialization_watches: BTreeMap<EntityId, (u64, u16)>,
    materialized_receipts: BTreeMap<EntityId, (u64, u16)>,
    initial_ready: BTreeMap<SessionKey, bace_gameplay_api::CharacterBinding>,
    initial_materialized: BTreeMap<SessionKey, bace_gameplay_api::CharacterBinding>,
    summon_job: Option<Job<PreparedSummonResult>>,
    summon_ready: Option<ReadySummon>,
    detaching: BTreeMap<EntityId, disconnect::PortalDisconnectHandoff>,
    controls: BTreeMap<u64, bace_simulation::PortalResolution>,
    retry_controls: std::collections::BTreeSet<u64>,
    control_failures: BTreeMap<
        SessionKey,
        (
            bace_gameplay_api::ActionContext,
            bace_interactions::PortalError,
        ),
    >,
}
impl PortalRuntime {
    pub(super) fn output_pending(&self) -> bool {
        !self.deliveries.is_empty()
    }
    pub(super) fn new() -> Self {
        Self {
            service: PortalService::new(),
            ticket: None,
            unmatched_resolution: None,
            deliveries: VecDeque::new(),
            tickets: BTreeMap::new(),
            blocked_effects: Default::default(),
            next: 0,
            publications: publications::PortalPublications::default(),
            transits: BTreeMap::new(),
            death_materialization_watches: BTreeMap::new(),
            materialized_receipts: BTreeMap::new(),
            initial_ready: Default::default(),
            initial_materialized: Default::default(),
            summon_job: None,
            summon_ready: None,
            detaching: BTreeMap::new(),
            controls: BTreeMap::new(),
            retry_controls: Default::default(),
            control_failures: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.publication_pending()
            || !self.detaching.is_empty()
            || !self.transits.is_empty()
            || !self.controls.is_empty()
            || self.service.requires_drain()
            || self.ticket.is_some()
            || self.unmatched_resolution.is_some()
            || !self.deliveries.is_empty()
            || !self.tickets.is_empty()
            || !self.blocked_effects.is_empty()
            || self.summon_job.is_some()
            || self.summon_ready.is_some()
    }
    fn holds(&self, actor: EntityId) -> bool {
        self.holds_without_transit(actor)
            || self.transits.contains_key(&actor)
            || self.detaching.contains_key(&actor)
    }
    fn push(&mut self, work: PortalDeliveryWork) {
        self.next += 1;
        self.deliveries.push_back(PortalDelivery {
            sequence: self.next,
            work,
            next_actor: 0,
        });
    }
    fn acknowledge(&mut self, sequence: u64) -> Result<(), String> {
        if self
            .deliveries
            .front()
            .is_none_or(|d| d.sequence != sequence)
        {
            return Err("portal delivery acknowledgment mismatch".into());
        }
        self.deliveries.pop_front();
        Ok(())
    }
    fn has_room(&self) -> bool {
        self.deliveries.len() < DELIVERY_CAPACITY && self.next < u64::MAX
    }
}
impl GameRuntime {
    pub(in crate::game_runtime) fn watch_death_portal_materialization(
        &mut self,
        actor: EntityId,
        operation: u64,
        accepted_epoch: u16,
    ) -> Result<(), String> {
        if actor.0 == 0 || operation == 0 || accepted_epoch == 0 {
            return Err("death portal materialization identity".into());
        }
        let expected = (operation, accepted_epoch);
        if let Some(old) = self.portals.death_materialization_watches.get(&actor) {
            return if *old == expected {
                Ok(())
            } else {
                Err("death portal materialization watch changed".into())
            };
        }
        if self.portals.death_materialization_watches.len() >= 4096 {
            return Err("death portal materialization watch capacity".into());
        }
        self.portals
            .death_materialization_watches
            .insert(actor, expected);
        Ok(())
    }
    /// Exact private reliable peer admission of the canonical materialization,
    /// or successful detached handoff after the peer disconnects.
    pub(in crate::game_runtime) fn death_portal_materialized(
        &self,
        actor: EntityId,
        operation: u64,
        accepted_epoch: u16,
    ) -> bool {
        self.portals.materialized_receipts.get(&actor) == Some(&(operation, accepted_epoch))
    }
    pub(in crate::game_runtime) fn forget_death_portal_materialization(
        &mut self,
        actor: EntityId,
        operation: u64,
        accepted_epoch: u16,
    ) {
        if self.portals.materialized_receipts.get(&actor) == Some(&(operation, accepted_epoch)) {
            self.portals.materialized_receipts.remove(&actor);
        }
        if self.portals.death_materialization_watches.get(&actor)
            == Some(&(operation, accepted_epoch))
        {
            self.portals.death_materialization_watches.remove(&actor);
        }
    }
    pub fn pending_portal_delivery(&self) -> Option<&PortalDelivery> {
        self.portals.deliveries.front()
    }
    pub fn acknowledge_portal_delivery(&mut self, sequence: u64) -> Result<(), String> {
        self.portals.acknowledge(sequence)
    }
    /// The caller first transfers the accepted summon blueprint to the
    /// canonical visibility owner. Only the exact retained event may be freed.
    pub fn acknowledge_summoned_portal(
        &mut self,
        entity: EntityId,
        template: u32,
    ) -> Result<(), String> {
        let Some(delivery) = self.portals.deliveries.front() else {
            return Err("summoned portal delivery missing".into());
        };
        let (sequence, operation) = match &delivery.work {
            PortalDeliveryWork::Event(PortalServiceEvent::Summoned {
                operation,
                entity: actual,
                template: actual_template,
            }) if *actual == entity && *actual_template == template => {
                (delivery.sequence, *operation)
            }
            _ => return Err("summoned portal handoff identity mismatch".into()),
        };
        self.portals.blocked_effects.remove(&operation);
        self.portals.acknowledge(sequence)
    }

    pub fn portal_failure(&self) -> Option<&str> {
        self.portals.service.blocked().or_else(|| {
            (!self.portals.blocked_effects.is_empty())
                .then_some("committed portal effect remains blocked")
        })
    }
    pub(super) fn portal_session_pending(&self, key: SessionKey) -> bool {
        self.sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .is_some_and(|l| self.portals.holds(l.loaded.binding.actor))
            && !self.disconnected_portal_handoff(key)
    }
    pub(super) fn retry_portal_session(&mut self, key: SessionKey) -> bool {
        if self.portal_session_pending(key) && self.portals.service.blocked().is_some() {
            self.portals.service.retry();
            true
        } else {
            false
        }
    }
    pub(super) fn poll_portals(&mut self) -> Result<(), String> {
        self.poll_portal_summons()?;
        self.project_portal_deliveries()?;
        self.poll_portal_controls()?;
        if self.portals.unmatched_resolution.is_some() {
            return Err("unmatched portal resolution retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.portal_resolutions().try_recv() else {
                break;
            };
            if self.portals.controls.contains_key(&outcome.correlation) {
                self.accept_portal_control(outcome)?;
                continue;
            }
            if let Err(outcome) = self.portals.service.accept_resolution(outcome) {
                self.portals.unmatched_resolution = Some(outcome);
                return Err("unmatched portal resolution retained".into());
            }
        }
        for _ in 0..self.limits.work_per_poll {
            if !self.portals.has_room() {
                break;
            }
            let Ok(event) = self.simulation.portal_events().try_recv() else {
                break;
            };
            self.portals.service.accept_effect(&event);
            self.portals.push(PortalDeliveryWork::Event(event));
        }
        if self.portals.has_room()
            && let Some(completion) = self.portals.service.take_completion()
        {
            if completion.work.ticket.origin == bace_simulation::PortalServiceOrigin::Spell {
                self.complete_magic_portal(
                    completion.work.ticket.cast_actor,
                    completion.work.ticket.cast,
                );
            }
            self.portals
                .push(PortalDeliveryWork::Completed(Box::new(completion)));
        }
        if !self.portals.service.requires_drain() && self.portals.has_room() {
            if self.portals.ticket.is_none() {
                self.portals.ticket = self.simulation.portal_proposals().try_recv().ok();
            }
            if let Some(ticket) = self.portals.ticket.as_ref() {
                let bindings = ticket
                    .participants
                    .iter()
                    .map(|(actor, _, _)| {
                        self.sessions
                            .values()
                            .filter_map(|s| s.loading.as_ref())
                            .find(|l| {
                                l.loaded.binding.actor == *actor
                                    && l.phase == lifecycle::Phase::Entered
                            })
                            .map(|l| l.loaded.binding)
                            .ok_or("portal participant has no entered session")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let ticket = self.portals.ticket.take().expect("checked portal ticket");
                let operation = ticket.operation;
                if self.portals.tickets.contains_key(&operation)
                    || self.portals.tickets.len() >= DELIVERY_CAPACITY
                {
                    self.portals.ticket = Some(ticket);
                    return Err("portal output ticket capacity or duplicate".into());
                }
                let retained = ticket.clone();
                if let Err(work) = self.portals.service.stage(PortalWork {
                    epoch: self.bootstrap.world_owner.epoch(),
                    bindings,
                    ticket,
                }) {
                    self.portals.ticket = Some(work.ticket);
                    return Err(
                        "portal proposal requires its composite owner or failed validation".into(),
                    );
                }
                self.portals.tickets.insert(operation, retained);
            }
        }
        let token = self.token()?;
        let input = self.simulation.input();
        self.portals.service.poll_with(
            token,
            &mut self.online_saves,
            &self.saves.handle,
            |command| input.try_submit(command).map_err(Box::new),
        )
    }
}

#[cfg(test)]
pub(in crate::game_runtime) mod tests;
