//! Corpse destruction remains reserved until its exact world-epoch tombstones
//! commit. The bounded SQL read runs outside the event pump and only supplies
//! immutable persisted baselines; the accepted ticket owns the current graph.
use super::*;
use crate::{
    corpse_expiry_saves::{CorpseExpiryFreezeInput, FrozenCorpseExpiry},
    placement_saves::PlacementResolution,
    saves::SaveSubmitError,
};
use bace_simulation::{CorpseExpiryEvent, CorpseExpiryTicket};
use bace_storage_codec::{CorpseSaveV5, ItemSaveV5};
mod spill;
struct Loaded {
    frozen: FrozenCorpseExpiry,
    prepared: Option<bace_simulation::PreparedCorpseSpill>,
    presentation: Option<Arc<CorpseSpillPresentation>>,
}
type SpillGeometry = (
    crate::region_activation::RegionAssetManifest,
    Arc<bace_physics::GeometryRegion>,
);
struct LoadedSources {
    corpse: CorpseSaveV5,
    rows: Vec<bace_persistence::LocatedSnapshot>,
    transient: Vec<crate::game_inventory::FrozenInventoryItem>,
    geometry: Option<SpillGeometry>,
}
struct Pending {
    ticket: CorpseExpiryTicket,
    frozen: Option<FrozenCorpseExpiry>,
    submitted: bool,
    correlation: Option<u64>,
    command: Option<PlayerDeathCommand>,
    acknowledged: bool,
    rejected: bool,
    event: Option<CorpseExpiryEvent>,
    spill_prepared: bool,
    presentation: Option<Arc<CorpseSpillPresentation>>,
}
pub(super) struct Expiry {
    pending: Option<Pending>,
    load: Option<Job<Result<Loaded, String>>>,
    blocked: Option<String>,
}
impl Expiry {
    pub fn new() -> Self {
        Self {
            pending: None,
            load: None,
            blocked: None,
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.load.is_some()
    }
    pub fn owns(&self, outcome: &PlayerDeathServiceOutcome) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.submitted && p.correlation == Some(outcome.correlation))
    }
    pub fn accept(&mut self, outcome: PlayerDeathServiceOutcome) {
        let p = self.pending.as_mut().expect("matched expiry outcome");
        match outcome.result {
            Ok(()) if !p.spill_prepared => {
                p.spill_prepared = true;
                p.submitted = false;
                p.correlation = None;
            }
            Ok(()) => p.acknowledged = true,
            Err((e, c)) => {
                p.command = Some(*c);
                p.submitted = false;
                self.blocked = Some(format!("corpse expiry adoption: {e:?}"));
            }
        }
    }
}
impl GameRuntime {
    pub fn retry_corpse_expiry(&mut self) -> Result<bool, String> {
        if self.deaths.expiry.blocked.is_none() {
            return Ok(false);
        }
        if let Some(p) = &mut self.deaths.expiry.pending
            && p.rejected
        {
            let frozen = p
                .frozen
                .as_mut()
                .ok_or("rejected corpse checkpoint missing")?;
            frozen.save = crate::placement_saves::PendingPlacementSave::new_world(
                bace_persistence::WorldPlacementOperation {
                    world_epoch: self.bootstrap.world_owner.epoch(),
                    inventory: frozen.save.operation().clone(),
                },
            )
            .map_err(|e| e.to_string())?;
            p.rejected = false;
        }
        self.deaths.expiry.blocked = None;
        Ok(true)
    }
    pub fn corpse_expiry_failure(&self) -> Option<&str> {
        self.deaths.expiry.blocked.as_deref()
    }
    pub(in crate::game_runtime) fn poll_corpse_expiry(&mut self, unix: u64) -> Result<(), String> {
        if self.deaths.unmatched_expiry.is_some() {
            return Err("unmatched corpse retirement retained".into());
        }
        if let Some(error) = &self.deaths.expiry.blocked {
            return Err(error.clone());
        }
        if self.deaths.expiry.pending.is_none()
            && let Ok(ticket) = self.simulation.corpse_expiry_proposals().try_recv()
        {
            self.deaths.expiry.pending = Some(Pending {
                ticket,
                frozen: None,
                submitted: false,
                correlation: None,
                command: None,
                acknowledged: false,
                rejected: false,
                event: None,
                spill_prepared: false,
                presentation: None,
            });
        }
        if self.deaths.expiry.pending.as_ref().is_some_and(|pending| {
            self.deaths
                .viewers
                .values()
                .any(|(corpse, _)| *corpse == pending.ticket.corpse)
                || self
                    .deaths
                    .access
                    .as_ref()
                    .is_some_and(|access| access.corpse == pending.ticket.corpse)
        }) {
            // The close packet and durable IsLooted handoff must finish before
            // retirement can publish removal to this viewer.
            return Ok(());
        }
        if let Some(result) = ready(&mut self.deaths.expiry.load) {
            match result {
                Ok(loaded) => {
                    let p = self
                        .deaths
                        .expiry
                        .pending
                        .as_mut()
                        .expect("retained expiry read");
                    p.frozen = Some(loaded.frozen);
                    p.command = loaded.prepared.map(PlayerDeathCommand::PrepareCorpseSpill);
                    p.spill_prepared = p.command.is_none();
                    p.presentation = loaded.presentation;
                }
                Err(error) => {
                    self.deaths.expiry.blocked = Some(error.clone());
                    return Err(error);
                }
            }
        }
        let delivery_room = self.deaths.room();
        let Some(p) = &mut self.deaths.expiry.pending else {
            return Ok(());
        };
        if p.event.is_none()
            && let Ok(event) = self.simulation.corpse_expiry_events().try_recv()
        {
            if event.corpse != p.ticket.corpse || event.death_operation != p.ticket.death_operation
            {
                self.deaths.unmatched_expiry = Some(event);
                return Err("unmatched corpse retirement retained".into());
            }
            p.event = Some(event);
        }
        if p.acknowledged && p.event.is_some() && delivery_room {
            if p.event
                .is_some_and(|e| e.phase == bace_simulation::CorpseExpiryPhase::Destroying)
            {
                let event = p.event.take().expect("checked decay phase");
                let ticket = p.ticket.clone();
                let spill = p.presentation.clone();
                self.deaths.push(DeathDeliveryWork::Expired {
                    event,
                    ticket,
                    spill,
                });
                return Ok(());
            }
            if !p.ticket.transient.is_empty() {
                let Some(world) = self.world.as_mut() else {
                    return Ok(());
                };
                world.regions.forget_transient_sources(&p.ticket.transient);
            }
            let p = self.deaths.expiry.pending.take().expect("finished expiry");
            self.deaths.push(DeathDeliveryWork::Expired {
                event: p.event.expect("checked expiry event"),
                ticket: p.ticket,
                spill: p.presentation,
            });
            return Ok(());
        }
        let p = self
            .deaths
            .expiry
            .pending
            .as_mut()
            .expect("retained expiry");
        if p.frozen.is_none() {
            if self.deaths.expiry.load.is_none() {
                let ticket = p.ticket.clone();
                let mut transient = Vec::with_capacity(ticket.transient.len());
                let mut transient_corpse = None;
                if !ticket.transient.is_empty() {
                    let Some(world) = self.world.as_ref() else {
                        return Ok(());
                    };
                    let mut bytes = 0usize;
                    for id in &ticket.transient {
                        let source = world
                            .regions
                            .transient_source(*id)
                            .ok_or("corpse descendant source archive missing")?;
                        bytes = bytes
                            .checked_add(
                                source
                                    .item
                                    .entity
                                    .encode_item()
                                    .map_err(|e| e.to_string())?
                                    .len(),
                            )
                            .ok_or("corpse source bytes overflow")?;
                        if bytes > 64 * 1024 * 1024 {
                            return Err("corpse source archive byte bound".into());
                        }
                        if source.item.persisted_version != 0 {
                            return Err("corpse transient archive already persisted".into());
                        }
                        if *id == ticket.corpse {
                            transient_corpse = source.corpse.clone();
                        }
                        transient.push(source.item.clone());
                    }
                }
                let store = self.bootstrap.store.clone();
                let epoch = self.bootstrap.world_owner.epoch();
                let geometry = if let Some(intent) = &ticket.spill {
                    let Some(world) = self.world.as_ref() else {
                        return Ok(());
                    };
                    Some(
                        world
                            .regions
                            .prepared_region((intent.position.cell >> 16) as u16)
                            .ok_or("corpse spill geometry missing")?
                            .geometry
                            .clone(),
                    )
                } else {
                    None
                };
                let manifest = self.bootstrap.assets.clone();
                self.deaths.expiry.load = Some(Box::pin(async move {
                    let root = store
                        .load(ticket.corpse.0)
                        .await
                        .map_err(|e| e.to_string())?;
                    let corpse = match (root, transient_corpse) {
                        (Some(row), None) => CorpseSaveV5::decode_or_migrate(&row.bytes, None)
                            .map_err(|e| e.to_string())?,
                        (None, Some(corpse)) => corpse,
                        _ => return Err("corpse durable/transient identity conflict".into()),
                    };
                    let bace_storage_codec::ItemPlacementV2::World(position) = &corpse.placement
                    else {
                        return Err("expiry root not in world".into());
                    };
                    let rows = store
                        .load_world_item_tree(
                            position.obj_cell_id,
                            bace_persistence::InventoryLoadLimits {
                                max_items: 4096,
                                max_depth: 64,
                                max_total_bytes: 64 * 1024 * 1024,
                            },
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    tokio::task::spawn_blocking(move || {
                        freeze_loaded(
                            epoch,
                            &ticket,
                            LoadedSources {
                                corpse,
                                rows,
                                transient,
                                geometry: geometry.map(|g| (manifest, g)),
                            },
                            unix,
                        )
                    })
                    .await
                    .map_err(|e| e.to_string())?
                }));
            }
            return Ok(());
        }
        if !p.spill_prepared {
            if p.correlation.is_none() {
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or("game correlation exhausted")?;
                p.correlation = Some(self.next);
            }
            if !p.submitted {
                let command = p.command.take().ok_or("corpse spill preparation missing")?;
                match self
                    .simulation
                    .input()
                    .try_submit(Command::PlayerDeathService(PlayerDeathServiceCommand {
                        correlation: p.correlation.expect("spill correlation"),
                        command,
                    })) {
                    Ok(()) => p.submitted = true,
                    Err(error) => {
                        let (closed, command) = match error {
                            TrySendError::Full(c) => (false, c),
                            TrySendError::Disconnected(c) => (true, c),
                        };
                        let Command::PlayerDeathService(command) = command else {
                            unreachable!()
                        };
                        p.command = Some(command.command);
                        if closed {
                            return Err("corpse spill owner closed".into());
                        }
                    }
                }
            }
            return Ok(());
        }
        if p.correlation.is_none() {
            let frozen = p.frozen.as_mut().expect("loaded expiry");
            if !p.submitted {
                match frozen.save.submit(&self.saves.handle) {
                    Ok(()) => p.submitted = true,
                    Err(SaveSubmitError::Full) => {}
                    Err(error) => return Err(error.to_string()),
                }
                return Ok(());
            }
            if let Some(outcome) = frozen.save.poll() {
                p.submitted = false;
                match outcome {
                    PlacementResolution::Committed(_) => {
                        p.command = Some(PlayerDeathCommand::ConfirmCorpseExpiry(
                            frozen.receipt.clone(),
                        ))
                    }
                    PlacementResolution::Rejected(error) => {
                        p.rejected = true;
                        self.deaths.expiry.blocked =
                            Some(format!("corpse expiry checkpoint rejected: {error}"));
                        return Err(self.deaths.expiry.blocked.clone().expect("set error"));
                    }
                    PlacementResolution::Uncertain(error) => {
                        self.deaths.expiry.blocked = Some(error.clone());
                        return Err(error);
                    }
                }
            } else {
                return Ok(());
            }
            self.next = self
                .next
                .checked_add(1)
                .ok_or("game correlation exhausted")?;
            p.correlation = Some(self.next);
        }
        let p = self
            .deaths
            .expiry
            .pending
            .as_mut()
            .expect("retained expiry");
        if !p.submitted {
            let correlation = p.correlation.expect("expiry correlation");
            let command = p.command.take().ok_or("expiry command missing")?;
            match self
                .simulation
                .input()
                .try_submit(Command::PlayerDeathService(PlayerDeathServiceCommand {
                    correlation,
                    command,
                })) {
                Ok(()) => p.submitted = true,
                Err(error) => {
                    let (closed, command) = match error {
                        TrySendError::Full(c) => (false, c),
                        TrySendError::Disconnected(c) => (true, c),
                    };
                    let Command::PlayerDeathService(command) = command else {
                        unreachable!()
                    };
                    p.command = Some(command.command);
                    if closed {
                        return Err("expiry owner closed".into());
                    }
                }
            }
        }
        Ok(())
    }
}
fn freeze_loaded(
    epoch: u64,
    ticket: &CorpseExpiryTicket,
    sources: LoadedSources,
    unix: u64,
) -> Result<Loaded, String> {
    let LoadedSources {
        mut corpse,
        rows,
        transient,
        geometry,
    } = sources;
    let mut items = Vec::with_capacity(ticket.inventory.proposal.changes.len());
    for change in &ticket.inventory.proposal.changes {
        let before = change
            .before
            .as_ref()
            .ok_or("expiry requires existing item")?;
        if ticket.transient.contains(&before.id) {
            if rows.iter().any(|r| r.aggregate.object_id == before.id.0) {
                return Err("transient expiry identity already durable".into());
            }
            let mut source = transient
                .iter()
                .find(|i| i.entity.object_id == before.id.0)
                .ok_or("transient expiry source missing")?
                .clone();
            if source.persisted_version != 0
                || source.entity.mutation_revision > before.revision
                || source.entity.state.weenie_id != before.template
            {
                return Err("stale transient expiry source".into());
            }
            source.entity.mutation_revision = before.revision;
            if before.id == ticket.corpse {
                corpse.corpse.entity = source.entity.clone();
            }
            items.push(source);
            continue;
        }
        let row = rows
            .iter()
            .find(|r| r.aggregate.object_id == before.id.0)
            .ok_or("expiry durable descendant missing")?;
        let (mut entity, placement, enchantments, construction, source_destination) =
            if before.id == ticket.corpse {
                let current = CorpseSaveV5::decode_or_migrate(&row.aggregate.bytes, None)
                    .map_err(|e| e.to_string())?;
                if current != corpse {
                    return Err("corpse changed during read".into());
                }
                (
                    current.corpse.entity.clone(),
                    current.placement.clone(),
                    current.enchantments.clone(),
                    None,
                    None,
                )
            } else {
                let item = ItemSaveV5::decode_or_migrate(&row.aggregate.bytes, None)
                    .map_err(|e| e.to_string())?;
                (
                    item.entity.clone(),
                    item.placement.clone(),
                    item.enchantments.clone(),
                    item.construction.clone(),
                    item.source_destination,
                )
            };
        if entity.mutation_revision > before.revision || entity.state.weenie_id != before.template {
            return Err("stale expiry before state".into());
        }
        // Timer-only dirty revisions may advance on unowned items. These rows
        // become tombstones; no live registry is reconstructed from this view.
        entity.mutation_revision = before.revision;
        if before.id == ticket.corpse {
            corpse.corpse.entity = entity.clone();
        }
        items.push(crate::game_inventory::FrozenInventoryItem {
            corpse: None,
            construction,
            source_destination,
            entity,
            placement: Some(placement),
            enchantments,
            persisted_version: row.aggregate.persisted_version,
        });
    }
    for item in &mut items {
        if let Some(entries) = ticket.enchantments.get(&EntityId(item.entity.object_id)) {
            item.enchantments = entries
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            if item.entity.object_id == ticket.corpse.0 {
                corpse.enchantments = item.enchantments.clone();
            }
        }
    }
    let spill = spill::prepare(ticket, &items, geometry)?;
    let frozen = crate::corpse_expiry_saves::freeze_corpse_expiry(CorpseExpiryFreezeInput {
        world_epoch: epoch,
        ticket,
        corpse: &corpse,
        items: &items,
        unix_seconds: i64::try_from(unix / 1000).map_err(|_| "expiry clock overflow")?,
        spill_positions: &spill.positions,
    })?;
    let presentation = ticket.spill.as_ref().map(|_| {
        Arc::new(CorpseSpillPresentation {
            snapshots: frozen.save.operation().snapshots.clone(),
            visibility: spill.visibility,
        })
    });
    Ok(Loaded {
        frozen,
        prepared: spill.prepared,
        presentation,
    })
}
