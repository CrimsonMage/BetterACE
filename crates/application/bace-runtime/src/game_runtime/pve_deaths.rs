//! Native creature death is a retained valuable-operation owner. It freezes
//! exact accepted pose, loot and player credits before SQL, stages the complete
//! world forest in simulation, and publishes only after a durable receipt.
mod cold;
use super::*;
use crate::{
    placement_saves::{PendingPlacementSave, PlacementResolution},
    saves::SaveSubmitError,
};
use bace_simulation::{
    Command, DeathProposal, PlayerSnapshotOperation, PlayerSnapshotOutcome, PlayerSnapshotRequest,
    PveEvent, PveServiceAction, PveServiceCommand,
};
use bace_types::EntityId;
use std::sync::mpsc::TrySendError;
const LIMIT: usize = 64;

enum Phase {
    Barrier,
    Capture,
    Capturing(u64),
    Cold,
    Staging,
    StageSent(u64),
    Saving,
    Committing,
    CommitSent(u64),
    Finalizing,
    Finalized,
}
struct Pending {
    proposal: DeathProposal,
    phase: Phase,
    actors: Vec<EntityId>,
    captured: usize,
    players: Vec<crate::death_saves::DeathPlayer>,
    item_baselines: BTreeMap<EntityId, Vec<crate::game_inventory::FrozenInventoryItem>>,
    leases: Vec<bace_persistence::CharacterLease>,
    ids: Vec<EntityId>,
    prepared: Option<cold::Prepared>,
    save: Option<PendingPlacementSave>,
    save_submitted: bool,
    save_rejected: bool,
    durable: bool,
    cache_registered: bool,
    ledger_accepted: bool,
    visibility_registered: usize,
    committed_rows: Vec<bace_persistence::SaveSnapshot>,
}
pub(super) struct PveDeathRuntime {
    proposals: VecDeque<DeathProposal>,
    pending: Option<Pending>,
    cold: Option<Job<cold::Completion>>,
    unexpected: Option<bace_simulation::PveServiceOutcome>,
    events: VecDeque<PveEvent>,
    published: BTreeMap<u64, Vec<EntityId>>,
    blocked: Option<String>,
}
impl PveDeathRuntime {
    pub(super) fn new() -> Self {
        Self {
            proposals: VecDeque::new(),
            pending: None,
            cold: None,
            unexpected: None,
            events: VecDeque::new(),
            published: BTreeMap::new(),
            blocked: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.proposals.is_empty()
            || self.pending.is_some()
            || self.cold.is_some()
            || self.unexpected.is_some()
            || !self.events.is_empty()
            || !self.published.is_empty()
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Capturing(id) if id == outcome.correlation))
    }
}
impl GameRuntime {
    fn poll_pve_publications(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Some(event) = self.pve_deaths.events.front() else {
                break;
            };
            let (operation, roots) = match event {
                PveEvent::CorpseCreated { operation, corpse } => (*operation, vec![*corpse]),
                PveEvent::NoCorpseWorldDropsCreated {
                    operation, roots, ..
                } => (*operation, roots.clone()),
                // Respawn and decay need their own durable source/visibility
                // transcript. The FIFO keeps those obligations intact.
                PveEvent::Respawned { .. } | PveEvent::CorpseDecayed { .. } => break,
            };
            let Some(registered) = self.pve_deaths.published.get(&operation) else {
                break;
            };
            if registered != &roots {
                return Err("PVE committed visibility roots mismatch".into());
            }
            self.pve_deaths.published.remove(&operation);
            self.pve_deaths.events.pop_front();
        }
        Ok(())
    }
    pub fn pending_pve_event(&self) -> Option<&PveEvent> {
        self.pve_deaths.events.front()
    }
    pub fn acknowledge_pve_event(&mut self, expected: &PveEvent) -> Result<(), String> {
        if self.pve_deaths.events.front() != Some(expected) {
            return Err("PVE event acknowledgment mismatch".into());
        }
        if let PveEvent::CorpseCreated { operation, corpse } = expected {
            if self.pve_deaths.published.get(operation) != Some(&vec![*corpse]) {
                return Err("PVE corpse visibility has not registered".into());
            }
            self.pve_deaths.published.remove(operation);
        }
        if let PveEvent::NoCorpseWorldDropsCreated {
            operation, roots, ..
        } = expected
        {
            if self.pve_deaths.published.get(operation) != Some(roots) {
                return Err("PVE world-drop visibility has not registered".into());
            }
            self.pve_deaths.published.remove(operation);
        }
        self.pve_deaths.events.pop_front();
        Ok(())
    }
    pub fn pve_death_failure(&self) -> Option<&str> {
        self.pve_deaths.blocked.as_deref()
    }
    /// Retain the accepted proposal and exact allocated identities across a
    /// transient source, queue, or uncertain-receipt failure.
    pub fn retry_pve_death(&mut self) {
        self.pve_deaths.blocked = None;
        if let Some(pending) = self.pve_deaths.pending.as_mut()
            && matches!(pending.phase, Phase::Saving)
            && !pending.durable
            && pending.save_rejected
        {
            pending.save = None;
            pending.save_rejected = false;
        }
    }
    pub(super) fn accept_pve_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        let Some(p) = self.pve_deaths.pending.as_mut() else {
            return Err(outcome);
        };
        if !matches!(p.phase, Phase::Capturing(id) if id == outcome.correlation) {
            return Err(outcome);
        }
        let actor = p.actors[p.captured];
        let credit = p
            .proposal
            .experience_state
            .iter()
            .find(|(id, _)| *id == actor)
            .map(|(_, credit)| credit)
            .expect("prepared PVE reward participant");
        let Some((baseline, version, lease)) = self.online_saves.baseline(actor.0) else {
            return Err(outcome);
        };
        let Ok(snapshot) = &outcome.result else {
            self.pve_deaths.blocked = Some("PVE player capture rejected".into());
            p.phase = Phase::Capture;
            return Ok(());
        };
        let operation = p.proposal.social.as_ref().map_or(
            PlayerSnapshotOperation::PveDeath(p.proposal.operation),
            |ticket| PlayerSnapshotOperation::Allegiance(ticket.operation),
        );
        if snapshot.binding().actor != actor
            || snapshot.operation() != Some((operation, credit.before_revision))
        {
            return Err(outcome);
        }
        let Ok(saved) = crate::player_saves::freeze_player_operation_baseline(
            baseline,
            snapshot,
            operation,
            credit.before_revision,
            unix,
        ) else {
            self.pve_deaths.blocked = Some("PVE player source freeze failed".into());
            p.phase = Phase::Capture;
            return Ok(());
        };
        if p.proposal.social.as_ref().is_some_and(|ticket| {
            ticket
                .item_experience
                .iter()
                .any(|(reward, inventory)| reward.actor == actor && inventory.is_some())
        }) {
            let Ok(items) = self.online_saves.operation_inventory_baselines(snapshot) else {
                self.pve_deaths.blocked = Some("PVE item reward baseline freeze failed".into());
                p.phase = Phase::Capture;
                return Ok(());
            };
            p.item_baselines.insert(actor, items);
        }
        p.players.push(crate::death_saves::DeathPlayer {
            saved,
            persisted_version: version,
        });
        p.leases.push(lease);
        p.captured += 1;
        p.phase = if p.captured == p.actors.len() {
            Phase::Cold
        } else {
            Phase::Capture
        };
        Ok(())
    }
    pub(super) fn poll_pve_deaths(&mut self, elapsed: Duration, unix: u64) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            if self.pve_deaths.proposals.len() >= LIMIT {
                break;
            }
            let Ok(proposal) = self.simulation.death_proposals().try_recv() else {
                break;
            };
            self.pve_deaths.proposals.push_back(proposal);
        }
        for _ in 0..self.limits.work_per_poll {
            if self.pve_deaths.events.len() >= LIMIT {
                break;
            }
            let Ok(event) = self.simulation.pve_events().try_recv() else {
                break;
            };
            self.pve_deaths.events.push_back(event);
        }
        self.poll_pve_publications()?;
        if let Some(unexpected) = &self.pve_deaths.unexpected {
            return Err(format!(
                "unmatched PVE service outcome retained: {}",
                unexpected.correlation
            ));
        }
        if let Ok(outcome) = self.simulation.pve_service_outcomes().try_recv() {
            let Some(p) = self.pve_deaths.pending.as_mut() else {
                self.pve_deaths.unexpected = Some(outcome);
                return Err("unmatched PVE service outcome".into());
            };
            let stage = matches!(p.phase, Phase::StageSent(id) if id == outcome.correlation);
            let commit = matches!(p.phase, Phase::CommitSent(id) if id == outcome.correlation);
            if !stage && !commit {
                self.pve_deaths.unexpected = Some(outcome);
                return Err("PVE service correlation mismatch".into());
            }
            match outcome.result {
                Ok(()) if stage => p.phase = Phase::Saving,
                Ok(()) => p.phase = Phase::Finalizing,
                Err(error) if stage => {
                    self.pve_deaths.blocked = Some(format!("PVE world stage: {error:?}"));
                    p.phase = Phase::Cold;
                    p.prepared = None;
                }
                Err(error) => {
                    self.pve_deaths.blocked = Some(format!("PVE committed admission: {error:?}"));
                    p.phase = Phase::Committing;
                }
            }
        }
        if let Some(done) = ready(&mut self.pve_deaths.cold) {
            let p = self
                .pve_deaths
                .pending
                .as_mut()
                .ok_or("PVE cold owner missing")?;
            p.ids = done.ids;
            match done.result {
                Ok(prepared) => {
                    p.prepared = Some(prepared);
                    p.phase = Phase::Staging;
                }
                Err(error) => {
                    self.pve_deaths.blocked = Some(error);
                    p.phase = Phase::Cold;
                }
            }
        }
        if let Some(error) = &self.pve_deaths.blocked {
            return Err(error.clone());
        }
        if self.pve_deaths.pending.is_none()
            && let Some(proposal) = self.pve_deaths.proposals.pop_front()
        {
            let actors = proposal
                .experience_state
                .iter()
                .map(|(id, _)| *id)
                .collect();
            self.pve_deaths.pending = Some(Pending {
                proposal,
                phase: Phase::Barrier,
                actors,
                captured: 0,
                players: vec![],
                item_baselines: BTreeMap::new(),
                leases: vec![],
                ids: vec![],
                prepared: None,
                save: None,
                save_submitted: false,
                save_rejected: false,
                durable: false,
                cache_registered: false,
                ledger_accepted: false,
                visibility_registered: 0,
                committed_rows: vec![],
            });
        }
        let Some(mut p) = self.pve_deaths.pending.take() else {
            return Ok(());
        };
        let result = self.advance_pve_death(&mut p, elapsed, unix);
        if !matches!(p.phase, Phase::Finalized) {
            self.pve_deaths.pending = Some(p);
        }
        result
    }
    fn advance_pve_death(
        &mut self,
        p: &mut Pending,
        elapsed: Duration,
        unix: u64,
    ) -> Result<(), String> {
        match p.phase {
            Phase::Barrier => {
                let actors: Vec<_> = p.actors.iter().map(|id| id.0).collect();
                if actors.is_empty() {
                    p.phase = Phase::Cold;
                } else if self.online_saves.critical_ready(&actors)? {
                    self.online_saves.begin_critical(&actors)?;
                    p.phase = Phase::Capture;
                }
            }
            Phase::Capture => {
                let actor = p.actors[p.captured];
                let before = p.proposal.experience_state[p.captured].1.before_revision;
                let operation = p.proposal.social.as_ref().map_or(
                    PlayerSnapshotOperation::PveDeath(p.proposal.operation),
                    |ticket| PlayerSnapshotOperation::Allegiance(ticket.operation),
                );
                let binding = self
                    .sessions
                    .values()
                    .filter_map(|session| session.loading.as_ref())
                    .find(|loading| loading.loaded.binding.actor == actor)
                    .map(|loading| loading.loaded.binding)
                    .ok_or("PVE reward participant session missing")?;
                let correlation = self.token()?;
                match self.simulation.input().try_submit(Command::PlayerSnapshot(
                    PlayerSnapshotRequest {
                        correlation,
                        binding,
                        operation: Some((operation, before)),
                    },
                )) {
                    Ok(()) => p.phase = Phase::Capturing(correlation),
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("PVE capture owner closed".into());
                    }
                }
            }
            Phase::Cold => {
                if self.pve_deaths.cold.is_some() {
                    return Ok(());
                }
                let position = p
                    .proposal
                    .position
                    .as_ref()
                    .ok_or("PVE accepted pose missing")?;
                let block = (position.obj_cell_id >> 16) as u16;
                let Some(region) = self
                    .world
                    .as_ref()
                    .and_then(|w| w.regions.prepared_region(block))
                    .cloned()
                else {
                    return Ok(());
                };
                let input = cold::Input {
                    proposal: p.proposal.clone(),
                    ids: p.ids.clone(),
                    generation: self.bootstrap.pack.generation.clone(),
                    region,
                    manifest: self.bootstrap.assets.clone(),
                    epoch: self.bootstrap.world_owner.epoch(),
                    unix_seconds: i64::try_from(unix / 1000).map_err(|_| "PVE Unix overflow")?,
                    tick: u64::try_from(elapsed.as_nanos() * 30 / 1_000_000_000)
                        .map_err(|_| "PVE tick overflow")?,
                    players: p
                        .players
                        .iter()
                        .map(|player| crate::death_saves::DeathPlayer {
                            saved: player.saved.clone(),
                            persisted_version: player.persisted_version,
                        })
                        .collect(),
                    leases: p.leases.clone(),
                    item_baselines: p.item_baselines.clone(),
                    social_before: p
                        .proposal
                        .social
                        .as_ref()
                        .map(|ticket| self.allegiance_before_images(ticket))
                        .transpose()?,
                };
                let store = self.bootstrap.store.clone();
                self.pve_deaths.cold = Some(Box::pin(cold::prepare(store, input)));
            }
            Phase::Staging => {
                let prepared = p.prepared.as_mut().ok_or("PVE frozen stage missing")?;
                let correlation = self.token()?;
                let forest = std::mem::take(&mut prepared.forest);
                let command = Command::PveService(PveServiceCommand {
                    correlation,
                    action: PveServiceAction::Stage {
                        operation: p.proposal.operation,
                        forest: Box::new(forest),
                    },
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = Phase::StageSent(correlation),
                    Err(error) => {
                        let (closed, command) = match error {
                            TrySendError::Full(command) => (false, command),
                            TrySendError::Disconnected(command) => (true, command),
                        };
                        let Command::PveService(PveServiceCommand {
                            action: PveServiceAction::Stage { forest, .. },
                            ..
                        }) = command
                        else {
                            unreachable!()
                        };
                        prepared.forest = *forest;
                        if closed {
                            return Err("PVE stage owner closed".into());
                        }
                    }
                }
            }
            Phase::Saving => {
                let prepared = p.prepared.as_ref().ok_or("PVE frozen save missing")?;
                if p.save.is_none() {
                    p.save = Some(
                        if let Some(allegiance) = &prepared.allegiance {
                            PendingPlacementSave::new_allegiance(
                                bace_persistence::AllegiancePlacementOperation {
                                    placement: prepared.operation.clone(),
                                    allegiance: allegiance.clone(),
                                    world_epoch: self.bootstrap.world_owner.epoch(),
                                    workflow: None,
                                },
                            )
                        } else {
                            PendingPlacementSave::new_world(
                                bace_persistence::WorldPlacementOperation {
                                    world_epoch: self.bootstrap.world_owner.epoch(),
                                    inventory: prepared.operation.clone(),
                                },
                            )
                        }
                        .map_err(|e| e.to_string())?,
                    );
                }
                let save = p.save.as_mut().expect("created PVE save");
                if !p.save_submitted {
                    match save.submit(&self.saves.handle) {
                        Ok(()) => p.save_submitted = true,
                        Err(SaveSubmitError::Full) => {}
                        Err(error) => return Err(error.to_string()),
                    }
                    return Ok(());
                }
                match save.poll() {
                    None => {}
                    Some(PlacementResolution::Uncertain(error)) => {
                        p.save_submitted = false;
                        return Err(error);
                    }
                    Some(PlacementResolution::Rejected(error)) => {
                        p.save_submitted = false;
                        p.save_rejected = true;
                        return Err(format!("PVE death rejected: {error}"));
                    }
                    Some(PlacementResolution::Committed(_)) => {
                        p.durable = true;
                        p.committed_rows = prepared.operation.snapshots.clone();
                        for row in &mut p.committed_rows {
                            row.expected_version = row
                                .expected_version
                                .checked_add(1)
                                .ok_or("PVE version overflow")?;
                        }
                        p.phase = Phase::Committing;
                    }
                }
            }
            Phase::Committing => {
                let prepared = p.prepared.as_ref().ok_or("PVE committed source missing")?;
                if !p.cache_registered {
                    let position = p
                        .proposal
                        .position
                        .as_ref()
                        .ok_or("PVE accepted pose missing")?;
                    let block = (position.obj_cell_id >> 16) as u16;
                    let world = self
                        .world
                        .as_mut()
                        .ok_or("PVE source cache owner unavailable")?;
                    if p.proposal.no_corpse {
                        world
                            .regions
                            .record_committed_world_item_sources(block, prepared.sources.clone())?;
                    } else {
                        world
                            .regions
                            .record_committed_corpse_sources(block, prepared.sources.clone())?;
                    }
                    p.cache_registered = true;
                }
                let correlation = self.token()?;
                let corpse = (!p.proposal.no_corpse).then_some(p.ids[0]);
                let items = p.ids[usize::from(!p.proposal.no_corpse)..p.ids.len() - 1].to_vec();
                let command = Command::PveService(PveServiceCommand {
                    correlation,
                    action: PveServiceAction::Committed {
                        operation: p.proposal.operation,
                        corpse,
                        items,
                        credits: p.proposal.experience_state.clone(),
                        social: p.proposal.social.clone().map(Box::new),
                    },
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = Phase::CommitSent(correlation),
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("PVE commit owner closed".into());
                    }
                }
            }
            Phase::Finalizing => {
                let prepared = p
                    .prepared
                    .as_ref()
                    .ok_or("PVE finalization source missing")?;
                if !p.cache_registered {
                    return Err("PVE finalization source cache not admitted".into());
                }
                if p.visibility_registered < prepared.visibility.len() {
                    let visible = &prepared.visibility[p.visibility_registered];
                    if !prepared.sources.iter().any(|source| {
                        source.item.entity.object_id == visible.description.object_id
                            && source.item.persisted_version == visible.revision as i64
                            && matches!(
                                source.item.placement,
                                Some(bace_storage_codec::ItemPlacementV2::World(_))
                            )
                    }) {
                        return Err("PVE root visibility source receipt mismatch".into());
                    }
                    let tick = u64::try_from(elapsed.as_nanos().saturating_mul(30) / 1_000_000_000)
                        .map_err(|_| "PVE visibility tick overflow")?;
                    visible
                        .register(&mut self.visibility.service, tick)
                        .map_err(|e| format!("PVE root visibility registration: {e:?}"))?;
                    p.visibility_registered += 1;
                    return Ok(());
                }
                if let Some(ticket) = &p.proposal.social
                    && !p.ledger_accepted
                {
                    let allegiance = p
                        .prepared
                        .as_ref()
                        .and_then(|prepared| prepared.allegiance.as_ref())
                        .ok_or("PVE shared committed ledger missing")?;
                    self.accept_source_allegiance_commit(ticket, allegiance)?;
                    p.ledger_accepted = true;
                }
                let actors: Vec<_> = p.actors.iter().map(|id| id.0).collect();
                if !actors.is_empty() {
                    self.online_saves
                        .finish_critical_for(&actors, &p.committed_rows)?;
                }
                if self.pve_deaths.published.len() >= LIMIT {
                    return Err("PVE publication acknowledgment capacity".into());
                }
                self.pve_deaths.published.insert(
                    p.proposal.operation,
                    prepared
                        .visibility
                        .iter()
                        .map(|visible| EntityId(visible.description.object_id))
                        .collect(),
                );
                p.phase = Phase::Finalized;
            }
            Phase::Finalized => {}
            Phase::Capturing(_) | Phase::StageSent(_) | Phase::CommitSent(_) => {}
        }
        Ok(())
    }
}
