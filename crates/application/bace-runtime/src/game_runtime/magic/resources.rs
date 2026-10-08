//! One critical resource lane. The immutable component/mana operation survives
//! save pressure, disconnect and uncertain commit; only exact receipts release it.
use super::*;
use crate::magic_saves::{
    MagicOperationId, MagicSaveInput, MagicSaveResolution, PendingMagicSave, freeze_magic_inventory,
};
use bace_persistence::SaveSnapshot;
use bace_simulation::{
    MagicResourceAction as Action, MagicResourceCommand, MagicResourceOutcome,
    MagicResourceResult as ResultKind, PreparedMagicResources,
};
use rand_core::{OsRng, RngCore};
pub(super) struct Resources {
    queue: VecDeque<(SessionKey, CharacterBinding, u64)>,
    pending: Option<PendingResource>,
    unexpected: Option<Arc<MagicResourceOutcome>>,
    pub(super) completion: Option<Completion>,
}
struct PendingResource {
    key: SessionKey,
    binding: CharacterBinding,
    cast: u64,
    id: MagicOperationId,
    token: u64,
    phase: Phase,
    ticket: Option<Arc<PreparedMagicResources>>,
    committed: Option<Vec<SaveSnapshot>>,
}
enum Phase {
    Barrier,
    Inspect,
    WaitingInspect,
    Saving {
        save: Box<PendingMagicSave>,
        submitted: bool,
    },
    Resolve,
    WaitingResolve,
    Baseline,
    Acknowledge,
    WaitingAck,
}
pub(super) struct Completion {
    pub(super) key: SessionKey,
    pub(super) binding: CharacterBinding,
    pub(super) ticket: Arc<PreparedMagicResources>,
    pub(super) committed: bool,
}
impl Resources {
    pub(super) fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            pending: None,
            unexpected: None,
            completion: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
            || !self.queue.is_empty()
            || self.unexpected.is_some()
            || self.completion.is_some()
    }
    pub(super) fn owns_session(&self, key: SessionKey) -> bool {
        self.queue.iter().any(|v| v.0 == key)
            || self.pending.as_ref().is_some_and(|p| p.key == key)
            || self.completion.as_ref().is_some_and(|p| p.key == key)
    }
}
impl GameRuntime {
    pub(super) fn queue_magic_resources(
        &mut self,
        actor: bace_types::EntityId,
        cast: u64,
    ) -> Result<bool, String> {
        if self
            .magic
            .resources
            .queue
            .iter()
            .any(|v| v.1.actor == actor && v.2 == cast)
            || self
                .magic
                .resources
                .pending
                .as_ref()
                .is_some_and(|p| p.binding.actor == actor && p.cast == cast)
        {
            return Ok(true);
        }
        if self.magic.resources.queue.len() >= 64 {
            return Ok(false);
        }
        let (key, binding) = self
            .sessions
            .iter()
            .find_map(|(key, s)| {
                s.loading
                    .as_ref()
                    .filter(|l| l.loaded.binding.actor == actor)
                    .map(|l| (*key, l.loaded.binding))
            })
            .ok_or("component request has no admitted player owner")?;
        self.magic.resources.queue.push_back((key, binding, cast));
        Ok(true)
    }
    pub(super) fn poll_magic_resources(&mut self, unix: u64) -> Result<(), String> {
        if self.magic.resources.unexpected.is_some() {
            return Err("unrelated magic resource receipt retained".into());
        }
        if self.magic.resources.pending.is_none()
            && self.magic.resources.completion.is_none()
            && let Some(&(key, binding, cast)) = self.magic.resources.queue.front()
        {
            let mut bytes = [0; 16];
            OsRng
                .try_fill_bytes(&mut bytes)
                .map_err(|e| e.to_string())?;
            let id = MagicOperationId::new(bytes).map_err(|e| e.to_string())?;
            let token = self.token()?;
            self.magic.resources.pending = Some(PendingResource {
                key,
                binding,
                cast,
                id,
                token,
                phase: Phase::Barrier,
                ticket: None,
                committed: None,
            });
            self.magic.resources.queue.pop_front();
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(done) = self.simulation.magic_resource_outcomes().try_recv() else {
                break;
            };
            if self.accept_magic_identity_pool(&done, unix)? {
                continue;
            }
            if let Err(done) = self.accept_magic_resource(done, unix) {
                self.magic.resources.unexpected = Some(done);
                return Err("magic resource correlation".into());
            }
        }
        let Some(p) = self.magic.resources.pending.as_mut() else {
            return Ok(());
        };
        let mut action = None;
        match &mut p.phase {
            Phase::Barrier => {
                if self.online_saves.critical_ready(&[p.binding.actor.0])? {
                    self.online_saves.begin_critical(&[p.binding.actor.0])?;
                    p.phase = Phase::Inspect;
                }
            }
            Phase::Inspect => action = Some(Action::Inspect { cast: p.cast }),
            Phase::Saving { save, submitted } => {
                if !*submitted {
                    match save.submit(&self.saves.handle) {
                        Ok(()) => *submitted = true,
                        Err(crate::saves::SaveSubmitError::Full) => {}
                        Err(error) => return Err(format!("magic save admission: {error:?}")),
                    }
                }
                if let Some(resolution) = save.poll() {
                    *submitted = false;
                    match resolution {
                        MagicSaveResolution::Committed {
                            acknowledgments, ..
                        } => {
                            let mut rows = save.operation().snapshots.clone();
                            for row in &mut rows {
                                let ack = acknowledgments
                                    .iter()
                                    .find(|a| a.object_id == row.object_id)
                                    .ok_or("magic save acknowledgment row")?;
                                row.expected_version = ack.persisted_version;
                            }
                            p.committed = Some(rows);
                            p.phase = Phase::Resolve;
                        }
                        MagicSaveResolution::Rejected { failure, .. } => {
                            self.magic.failures.insert(p.key, failure.to_string());
                            p.phase = Phase::Resolve;
                        }
                        MagicSaveResolution::Uncertain { message } => {
                            self.magic.failures.insert(p.key, message);
                        }
                    }
                }
            }
            Phase::Resolve => {
                action = Some(Action::Resolve {
                    operation: p
                        .ticket
                        .as_ref()
                        .ok_or("magic resource ticket missing")?
                        .inventory
                        .operation,
                    committed: p.committed.is_some(),
                })
            }
            Phase::Baseline => {
                if let Some(rows) = &p.committed {
                    self.online_saves
                        .finish_critical_for(&[p.binding.actor.0], rows)?;
                } else {
                    self.online_saves.cancel_critical(&[p.binding.actor.0])?;
                }
                p.phase = Phase::Acknowledge;
            }
            Phase::Acknowledge => {
                action = Some(Action::Acknowledge {
                    operation: p
                        .ticket
                        .as_ref()
                        .ok_or("magic resource ticket missing")?
                        .inventory
                        .operation,
                })
            }
            Phase::WaitingInspect | Phase::WaitingResolve | Phase::WaitingAck => {}
        }
        if let Some(action) = action {
            let command = Command::MagicResource(MagicResourceCommand {
                correlation: p.token,
                binding: Some(p.binding),
                action: action.clone(),
            });
            match self.simulation.input().try_submit(command) {
                Ok(()) => {
                    p.phase = match action {
                        Action::Inspect { .. } => Phase::WaitingInspect,
                        Action::Resolve { .. } => Phase::WaitingResolve,
                        Action::Acknowledge { .. } => Phase::WaitingAck,
                        Action::SupplyPortalIds(_) | Action::SupplyProjectileIds(_) => {
                            unreachable!("separate portal pool owner")
                        }
                    }
                }
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("magic resource owner ingress closed".into());
                }
            }
        }
        Ok(())
    }
    fn accept_magic_resource(
        &mut self,
        done: Arc<MagicResourceOutcome>,
        unix: u64,
    ) -> Result<(), Arc<MagicResourceOutcome>> {
        let Some(p) = self.magic.resources.pending.as_mut() else {
            return Err(done);
        };
        if p.token != done.correlation || Some(p.binding) != done.binding {
            return Err(done);
        }
        match (&p.phase, &done.result) {
            (Phase::WaitingInspect, Ok(ResultKind::Prepared(ticket)))
                if ticket.resources.actor == p.binding.actor
                    && ticket.resources.cast == p.cast
                    && ticket.snapshot.binding() == p.binding =>
            {
                p.ticket = Some(ticket.clone());
                match freeze(ticket, p.id, &self.online_saves, unix) {
                    Ok(save) => {
                        p.phase = Phase::Saving {
                            save: Box::new(save),
                            submitted: false,
                        }
                    }
                    Err(error) => {
                        self.magic.failures.insert(p.key, error);
                        p.phase = Phase::Resolve;
                    }
                }
            }
            (Phase::WaitingInspect, Err(error)) => {
                self.magic
                    .failures
                    .insert(p.key, format!("magic resource inspection: {error:?}"));
                p.phase = Phase::Inspect;
            }
            (
                Phase::WaitingResolve,
                Ok(ResultKind::Resolved {
                    operation,
                    committed,
                }),
            ) if p
                .ticket
                .as_ref()
                .is_some_and(|t| t.inventory.operation == *operation)
                && *committed == p.committed.is_some() =>
            {
                p.phase = Phase::Baseline
            }
            (Phase::WaitingResolve, Err(error)) => {
                self.magic
                    .failures
                    .insert(p.key, format!("magic resource owner receipt: {error:?}"));
                p.phase = Phase::Resolve;
            }
            (Phase::WaitingAck, Ok(ResultKind::Acknowledged { operation }))
                if p.ticket
                    .as_ref()
                    .is_some_and(|t| t.inventory.operation == *operation) =>
            {
                let p = self
                    .magic
                    .resources
                    .pending
                    .take()
                    .expect("matched terminal");
                self.magic.resources.completion = Some(Completion {
                    key: p.key,
                    binding: p.binding,
                    ticket: p.ticket.expect("prepared receipt"),
                    committed: p.committed.is_some(),
                });
            }
            (Phase::WaitingAck, Err(error)) => {
                self.magic
                    .failures
                    .insert(p.key, format!("magic resource acknowledgment: {error:?}"));
                p.phase = Phase::Acknowledge;
            }
            _ => return Err(done),
        }
        Ok(())
    }
}
fn freeze(
    ticket: &PreparedMagicResources,
    id: MagicOperationId,
    online: &OnlinePlayerSaveService,
    unix: u64,
) -> Result<PendingMagicSave, String> {
    let resources = &ticket.resources;
    let snapshot = &ticket.snapshot;
    let actor = resources.actor.0;
    let (baseline, version, lease) = online
        .baseline(actor)
        .ok_or("magic player baseline missing")?;
    let player = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        snapshot,
        bace_simulation::PlayerSnapshotOperation::Inventory(ticket.inventory.operation),
        resources.before_revision,
        unix,
    )
    .map_err(|e| e.to_string())?;
    let mut items = online.operation_inventory_baselines(snapshot)?;
    for change in &ticket.inventory.proposal.changes {
        let before = change
            .before
            .as_ref()
            .ok_or("component burn cannot create an item")?;
        let source = items
            .iter_mut()
            .find(|i| i.entity.object_id == before.id.0)
            .ok_or("component source missing")?;
        if source.entity.mutation_revision > before.revision
            || source.entity.state.weenie_id != before.template
        {
            return Err("component before image changed".into());
        }
        source.entity.mutation_revision = before.revision;
        crate::game_inventory::set(
            &mut source.entity.state.properties.ints,
            12,
            i32::try_from(before.stack).map_err(|_| "component count")?,
        );
    }
    let mut rows = online.operation_inventory_changes(snapshot)?;
    rows.retain(|r| {
        !ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|c| c.after.id.0 == r.object_id)
    });
    // Capture's explicit server UTC pairs with snapshot.tick. Resource recovery
    // began earlier; subtract that authoritative simulation interval, never the
    // variable wall time spent waiting for PostgreSQL.
    let age = snapshot.tick() as f64 / 30. - resources.prepared_at;
    if !age.is_finite() || age < 0. || age * 1000. > u64::MAX as f64 {
        return Err("magic resource capture clock".into());
    }
    let captured = unix
        .checked_sub((age * 1000.).round() as u64)
        .ok_or("magic resource UTC underflow")?;
    freeze_magic_inventory(MagicSaveInput {
        id,
        ticket: &ticket.inventory,
        resources,
        player: &player,
        player_version: version,
        lease,
        captured_unix_millis: captured,
        inventory: crate::game_inventory::InventoryFreezeInput {
            operation_id: "validated-by-magic-id",
            proposal: &ticket.inventory.proposal,
            items: &items,
            other_snapshots: &rows,
            leases: &[lease],
            storage_views: &[],
            admitted_positions: &BTreeMap::new(),
        },
    })
    .map_err(|e| e.to_string())
}
