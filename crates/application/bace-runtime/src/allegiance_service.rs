//! Retained allegiance durability owner. SQL completion, simulation adoption and
//! online baseline handoff are distinct stages; uncertainty never releases holds.
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    saves::{SaveHandle, SaveSubmitError},
    simulation::SimulationInput,
};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{AllegianceOperation, SaveSnapshot, StoredAllegiance};
use bace_simulation::{
    AllegianceTicket, Command, PlayerReadSnapshot, PlayerSnapshotOperation, PlayerSnapshotOutcome,
    PlayerSnapshotRequest, SocialControl, SocialControlAction, SocialControlOutcome,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, mpsc::TrySendError},
};
mod freezing;
mod ledger;

enum Phase {
    Barrier,
    CaptureReady,
    Capturing {
        correlation: u64,
        actor: u32,
    },
    Saving {
        save: Box<freezing::Frozen>,
        submitted: bool,
    },
    Delivering {
        sequence: Option<u64>,
        committed: bool,
        operation: Option<Box<AllegianceOperation>>,
        rows: Vec<SaveSnapshot>,
    },
    Finishing {
        committed: bool,
        operation: Option<Box<AllegianceOperation>>,
        rows: Vec<SaveSnapshot>,
    },
}
pub(crate) struct NpcSharedCheckpoint {
    pub source_inventory: Option<std::sync::Arc<crate::npc_persistence::FrozenNpcSourceInventory>>,
    pub token: crate::npc_service::NpcExternalStage,
    pub checkpoint: bace_simulation::NpcSourceCheckpoint,
}
struct Pending {
    npc: Option<NpcSharedCheckpoint>,
    ticket: AllegianceTicket,
    leases: Vec<bace_persistence::CharacterLease>,
    bindings: BTreeMap<u32, CharacterBinding>,
    captures: BTreeMap<u32, (Arc<PlayerReadSnapshot>, u64)>,
    captured_bytes: usize,
    phase: Phase,
    reserved: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllegianceCompletion {
    pub npc: Option<crate::npc_service::NpcExternalStage>,
    pub operation: u64,
    pub committed: bool,
}
pub struct AllegianceService {
    epoch: u64,
    ledger: ledger::Ledger,
    pending: Option<Pending>,
    completion: Option<AllegianceCompletion>,
    blocked: Option<String>,
    last_correlation: u64,
}
impl AllegianceService {
    pub fn new(
        epoch: u64,
        nodes: &[StoredAllegiance],
        metadata: &[StoredAllegiance],
    ) -> Result<Self, String> {
        if epoch == 0 || epoch > i64::MAX as u64 {
            return Err("allegiance world epoch".into());
        }
        Ok(Self {
            epoch,
            ledger: ledger::Ledger::new(nodes, metadata)?,
            pending: None,
            completion: None,
            blocked: None,
            last_correlation: 0,
        })
    }
    /// NPC and corpse rewards require their source workflow/corpse composite;
    /// this standalone lane returns those tickets intact to that owner.
    pub fn stage(
        &mut self,
        ticket: AllegianceTicket,
        bindings: Vec<CharacterBinding>,
        leases: Vec<bace_persistence::CharacterLease>,
    ) -> Result<(), Box<AllegianceTicket>> {
        self.stage_inner(ticket, bindings, leases, None)
    }
    pub(crate) fn stage_npc(
        &mut self,
        ticket: AllegianceTicket,
        bindings: Vec<CharacterBinding>,
        leases: Vec<bace_persistence::CharacterLease>,
        npc: NpcSharedCheckpoint,
    ) -> Result<(), Box<(AllegianceTicket, NpcSharedCheckpoint)>> {
        if ticket
            .npc
            .as_ref()
            .is_none_or(|p| p.context.source.0 != npc.token.binding.source)
            || ticket.operation != npc.token.operation
        {
            return Err(Box::new((ticket, npc)));
        }
        // Preserve the exact source lock if ordinary participant admission rejects.
        let retained = NpcSharedCheckpoint {
            source_inventory: npc.source_inventory.clone(),
            token: npc.token,
            checkpoint: npc.checkpoint.clone(),
        };
        self.stage_inner(ticket, bindings, leases, Some(npc))
            .map_err(|ticket| Box::new((*ticket, retained)))
    }
    fn stage_inner(
        &mut self,
        ticket: AllegianceTicket,
        bindings: Vec<CharacterBinding>,
        leases: Vec<bace_persistence::CharacterLease>,
        npc: Option<NpcSharedCheckpoint>,
    ) -> Result<(), Box<AllegianceTicket>> {
        let actors: BTreeSet<_> = ticket.player_changes.iter().map(|(a, _)| a.0).collect();
        let supplied: BTreeMap<_, _> = bindings.iter().map(|b| (b.actor.0, *b)).collect();
        let participants = Self::participants(&ticket);
        let fences: BTreeSet<_> = leases.iter().map(|l| l.character_id).collect();
        if self.requires_drain()
            || ticket.operation == 0
            || ticket.npc.is_some() != npc.is_some()
            || ticket.rare.is_some()
            || ticket.patch.nodes.len() + ticket.patch.metadata.len() > 1024
            || ticket.player_changes.len() > 1024
            || ticket.item_experience.len() > 64
            || participants.len() > 1024
            || fences.len() != leases.len()
            || !participants.iter().eq(fences.iter())
            || leases.iter().any(|l| {
                l.epoch < 0
                    || !matches!(
                        l.state,
                        bace_persistence::OwnershipState::Online
                            | bace_persistence::OwnershipState::Offline
                    )
            })
            || actors.len() != ticket.player_changes.len()
            || supplied.len() != bindings.len()
            || !actors.iter().eq(supplied.keys())
            || bindings
                .iter()
                .any(|b| b.account.0 == 0 || b.session.0 == 0)
        {
            return Err(Box::new(ticket));
        }
        self.pending = Some(Pending {
            npc,
            ticket,
            leases,
            bindings: supplied,
            captures: BTreeMap::new(),
            captured_bytes: 0,
            phase: Phase::Barrier,
            reserved: false,
        });
        self.blocked = None;
        Ok(())
    }
    pub fn participants(ticket: &AllegianceTicket) -> Vec<u32> {
        ticket
            .player_changes
            .iter()
            .map(|(id, _)| id.0)
            .chain(
                ticket
                    .patch
                    .nodes
                    .iter()
                    .filter_map(|(b, a)| b.as_ref().or(a.as_ref()).map(|n| n.character.0)),
            )
            .chain(
                ticket
                    .patch
                    .metadata
                    .iter()
                    .filter_map(|(b, a)| b.as_ref().or(a.as_ref()).map(|n| n.monarch.0)),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn requires_drain(&self) -> bool {
        self.pending.is_some() || self.completion.is_some()
    }
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    /// Touched before-images for a corpse/NPC owner's larger atomic operation.
    /// The caller must retain that composite and hand back its exact committed
    /// ledger after its simulation owner acknowledges adoption.
    pub fn external_before_images(
        &self,
        ticket: &AllegianceTicket,
    ) -> Result<(Vec<StoredAllegiance>, Vec<StoredAllegiance>), String> {
        if self.requires_drain() || ticket.patch.nodes.len() + ticket.patch.metadata.len() > 1024 {
            return Err("allegiance baseline is reserved or oversized".into());
        }
        Ok(self.ledger.before(ticket))
    }
    pub fn accept_external_commit(
        &mut self,
        ticket: &AllegianceTicket,
        operation: &AllegianceOperation,
    ) -> Result<(), String> {
        let (nodes, metadata) = self.external_before_images(ticket)?;
        let expected = crate::allegiance_saves::freeze_allegiance(
            crate::allegiance_saves::AllegianceFreezeInput {
                world_epoch: self.epoch,
                operation: ticket.operation,
                patch: &ticket.patch,
                stored_nodes: &nodes,
                stored_metadata: &metadata,
                players: &[],
                leases: &operation.leases,
            },
        )
        .map_err(|e| e.to_string())?;
        if expected.nodes != operation.nodes || expected.metadata != operation.metadata {
            return Err("external allegiance committed patch mismatch".into());
        }
        self.ledger.adopt(operation)
    }
    pub fn take_completion(&mut self) -> Option<AllegianceCompletion> {
        self.completion.take()
    }
    pub fn retry(&mut self) {
        self.blocked = None;
    }
    pub fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.pending.as_ref().is_some_and(|p|matches!(p.phase,Phase::Capturing{correlation,..} if correlation==outcome.correlation))
    }
    pub fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix_millis: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched capture");
        let Phase::Capturing { actor, .. } = p.phase else {
            unreachable!()
        };
        match outcome.result {
            Ok(snapshot) => {
                let expected = p
                    .ticket
                    .player_changes
                    .iter()
                    .find(|(id, _)| id.0 == actor)
                    .expect("capture actor")
                    .1
                    .experience
                    .before_revision;
                if p.bindings.get(&actor) != Some(&snapshot.binding())
                    || snapshot.operation()
                        != Some((
                            PlayerSnapshotOperation::Allegiance(p.ticket.operation),
                            expected,
                        ))
                {
                    self.blocked = Some("allegiance capture identity mismatch".into());
                    p.captures.insert(actor, (snapshot, unix_millis));
                } else {
                    // Each captured inventory is bounded by the simulation. A
                    // byte ceiling is checked again on the frozen aggregates.
                    let entries = snapshot.enchantments().map_or(0, |r| r.entries().len())
                        + snapshot
                            .item_enchantments()
                            .iter()
                            .map(|(_, r)| r.entries().len())
                            .sum::<usize>();
                    p.captured_bytes = p
                        .captured_bytes
                        .saturating_add(64 * 1024)
                        .saturating_add(std::mem::size_of_val(snapshot.items()))
                        .saturating_add(
                            entries * std::mem::size_of::<bace_magic::EnchantmentEntry>(),
                        );
                    p.captures.insert(actor, (snapshot, unix_millis));
                    if p.captured_bytes > 64 * 1024 * 1024 {
                        self.blocked = Some("allegiance capture budget".into());
                    }
                }
                p.phase = Phase::CaptureReady;
            }
            Err(error) => {
                p.phase = Phase::CaptureReady;
                self.blocked = Some(format!("allegiance capture: {error:?}"));
            }
        }
        Ok(())
    }
    pub fn owns_control(&self, outcome: &SocialControlOutcome) -> bool {
        self.pending.as_ref().is_some_and(|p|matches!(p.phase,Phase::Delivering{sequence:Some(sequence),..} if sequence==outcome.sequence))
    }
    pub fn accept_control(
        &mut self,
        outcome: SocialControlOutcome,
    ) -> Result<(), SocialControlOutcome> {
        if !self.owns_control(&outcome) {
            return Err(outcome);
        }
        let p = self.pending.as_mut().expect("matched control");
        if outcome.result != Ok(Some(p.ticket.operation)) {
            self.blocked = Some(format!("allegiance owner receipt: {:?}", outcome.result));
            if let Phase::Delivering { sequence, .. } = &mut p.phase {
                *sequence = None;
            }
            return Ok(());
        }
        let Phase::Delivering {
            committed,
            operation,
            rows,
            ..
        } = &mut p.phase
        else {
            unreachable!()
        };
        p.phase = Phase::Finishing {
            committed: *committed,
            operation: operation.take(),
            rows: std::mem::take(rows),
        };
        Ok(())
    }
    /// Only a never-submitted proposal may be cancelled. Retains all ownership
    /// until the simulation acknowledges the exact rejection ticket.
    pub fn reject_unsubmitted(&mut self) -> Result<(), String> {
        let p = self.pending.as_mut().ok_or("no allegiance proposal")?;
        if !matches!(p.phase, Phase::Barrier | Phase::CaptureReady) {
            return Err("allegiance capture/write must resolve before cancellation".into());
        }
        p.phase = Phase::Delivering {
            sequence: None,
            committed: false,
            operation: None,
            rows: vec![],
        };
        self.blocked = None;
        Ok(())
    }
    pub fn poll(
        &mut self,
        input: &SimulationInput,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
        correlation: u64,
    ) -> Result<(), String> {
        self.poll_with(online, saves, correlation, |command| {
            input.try_submit(command).map_err(Box::new)
        })
    }
    fn poll_with(
        &mut self,
        online: &mut OnlinePlayerSaveService,
        saves: &SaveHandle,
        correlation: u64,
        mut send: impl FnMut(Command) -> Result<(), Box<TrySendError<Command>>>,
    ) -> Result<(), String> {
        if let Some(error) = &self.blocked {
            return Err(error.clone());
        }
        let Some(p) = &mut self.pending else {
            return Ok(());
        };
        let actors: Vec<_> = p.bindings.keys().copied().collect();
        let mut complete = false;
        let result = (|| -> Result<(), String> {
            match &mut p.phase {
                Phase::Barrier => {
                    if !actors.is_empty() {
                        if !online.critical_ready(&actors)? {
                            return Ok(());
                        }
                        online.begin_critical(&actors)?;
                        p.reserved = true;
                    }
                    p.phase = Phase::CaptureReady;
                }
                Phase::CaptureReady => {
                    if let Some((&actor, &binding)) = p
                        .bindings
                        .iter()
                        .find(|(actor, _)| !p.captures.contains_key(actor))
                    {
                        if correlation == 0 || correlation <= self.last_correlation {
                            return Err("stale allegiance correlation".into());
                        }
                        let revision = p
                            .ticket
                            .player_changes
                            .iter()
                            .find(|(id, _)| id.0 == actor)
                            .expect("checked actor")
                            .1
                            .experience
                            .before_revision;
                        let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                            correlation,
                            binding,
                            operation: Some((
                                PlayerSnapshotOperation::Allegiance(p.ticket.operation),
                                revision,
                            )),
                        });
                        match send(command) {
                            Ok(()) => {
                                self.last_correlation = correlation;
                                p.phase = Phase::Capturing { correlation, actor };
                            }
                            Err(error) => {
                                if matches!(*error, TrySendError::Disconnected(_)) {
                                    return Err("allegiance snapshot ingress closed".into());
                                }
                            }
                        }
                    } else {
                        let save = freezing::freeze(
                            self.epoch,
                            &self.ledger,
                            &p.ticket,
                            &p.captures,
                            &p.leases,
                            online,
                            p.npc.as_ref(),
                        )?;
                        p.phase = Phase::Saving {
                            save: Box::new(save),
                            submitted: false,
                        };
                    }
                }
                Phase::Capturing { .. } => {}
                Phase::Saving { save, submitted } => {
                    if !*submitted {
                        match save.submit(saves) {
                            Ok(()) => *submitted = true,
                            Err(SaveSubmitError::Full) => {}
                            Err(error) => return Err(error.to_string()),
                        }
                        return Ok(());
                    }
                    if let Some(resolution) = save.poll() {
                        *submitted = false;
                        let committed = resolution?;
                        if !committed && p.npc.is_some() {
                            **save = save.retry_rejected_npc()?;
                            return Err("NPC shared stage rejected; exact composite and source hold retained for retry".into());
                        }
                        let operation = committed.then(|| Box::new(save.operation().clone()));
                        let rows = if committed {
                            save.committed_rows()?
                        } else {
                            vec![]
                        };
                        p.phase = Phase::Delivering {
                            sequence: None,
                            committed,
                            operation,
                            rows,
                        };
                    }
                }
                Phase::Delivering {
                    sequence,
                    committed,
                    ..
                } => {
                    if sequence.is_none() {
                        if correlation == 0 || correlation <= self.last_correlation {
                            return Err("stale allegiance receipt correlation".into());
                        }
                        let action = if *committed {
                            SocialControlAction::Commit(Box::new(p.ticket.clone()))
                        } else {
                            SocialControlAction::Reject(Box::new(p.ticket.clone()))
                        };
                        match send(Command::SocialControl(SocialControl {
                            sequence: correlation,
                            action,
                        })) {
                            Ok(()) => {
                                *sequence = Some(correlation);
                                self.last_correlation = correlation;
                            }
                            Err(error) => {
                                if matches!(*error, TrySendError::Disconnected(_)) {
                                    return Err("allegiance owner ingress closed".into());
                                }
                            }
                        }
                    }
                }
                Phase::Finishing {
                    committed,
                    operation,
                    rows,
                } => {
                    if *committed {
                        let operation = operation
                            .as_ref()
                            .ok_or("missing allegiance frozen operation")?;
                        self.ledger.validate(operation)?;
                        if p.reserved {
                            online.finish_critical_for(&actors, rows)?;
                        }
                        self.ledger.adopt(operation)?;
                    } else if p.reserved {
                        online.cancel_critical(&actors)?;
                    }
                    self.completion = Some(AllegianceCompletion {
                        npc: p.npc.as_ref().map(|n| n.token),
                        operation: p.ticket.operation,
                        committed: *committed,
                    });
                    complete = true;
                }
            }
            Ok(())
        })();
        if complete {
            self.pending = None;
        }
        if let Err(error) = &result {
            self.blocked = Some(error.clone());
        }
        result
    }
}
#[cfg(test)]
mod postgres_tests;
#[cfg(test)]
mod tests;
