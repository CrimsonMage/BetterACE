//! Epoch-fenced lifecycle tombstones retain their exact operation on uncertainty.
use super::*;
use crate::{
    game_inventory::{FrozenInventoryItem, InventoryFreezeInput},
    generated_retirement::{GeneratedRetirementResolution, PendingGeneratedRetirement},
};
use bace_inventory::ItemPlace;
use bace_storage_codec::ItemPlacementV2;
pub(super) struct Retiring {
    ticket: bace_simulation::GeneratedRetirementTicket,
    sources: Vec<FrozenInventoryItem>,
    bytes: usize,
    next: usize,
    pending: Option<PendingGeneratedRetirement>,
    submitted: bool,
    command: Option<GeneratorAction>,
    delivery: Option<delivery::Delivery>,
    rejected: bool,
    complete: bool,
    blocked: Option<String>,
}
impl Retiring {
    pub fn delivery(&self) -> Option<&delivery::Delivery> {
        self.delivery.as_ref()
    }
    pub fn failure(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    pub fn retry(&mut self) {
        self.blocked = None;
        if self.rejected {
            self.command = Some(GeneratorAction::RejectRetirement {
                operation: self.ticket.inventory.operation,
            });
        }
    }
    pub fn accept(&mut self, outcome: GeneratorCommandOutcome) {
        match outcome.result {
            Ok(()) => {
                self.delivery = None;
                self.complete = true;
            }
            Err(GeneratorServiceError::Capacity | GeneratorServiceError::Busy) => {
                self.delivery.as_mut().unwrap().submitted = false;
            }
            Err(e) => {
                self.delivery.as_mut().unwrap().submitted = false;
                self.blocked = Some(format!("generator retirement owner receipt: {e:?}"));
            }
        }
    }
}
impl<S: GeneratorRepository> GeneratorService<S> {
    pub(super) async fn poll_retirement(
        &mut self,
        worker: &SimulationWorker,
        regions: &mut impl GeneratorRegions,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        if self.retirement.as_ref().is_some_and(|r| r.complete) {
            let retired = self.retirement.take().ok_or("retirement disappeared")?;
            if !retired.rejected {
                regions.forget_transient_sources(
                    &retired
                        .ticket
                        .inventory
                        .proposal
                        .changes
                        .iter()
                        .map(|c| c.after.id)
                        .collect::<Vec<_>>(),
                );
            }
        }
        if self.retirement.is_none() {
            let ticket = match worker.generator_retirements().try_recv() {
                Ok(t) => t,
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if self.quiescing {
                        return Ok(());
                    }
                    return Err("generator retirement lane disconnected".into());
                }
            };
            self.retirement = Some(Retiring {
                ticket,
                sources: vec![],
                bytes: 0,
                next: 0,
                pending: None,
                submitted: false,
                command: None,
                delivery: None,
                rejected: false,
                complete: false,
                blocked: None,
            });
        }
        if let Some(r) = &mut self.retirement
            && (r.ticket.inventory.proposal.changes.is_empty()
                || r.ticket.inventory.proposal.changes.len() > 1024)
        {
            r.blocked = Some("generator retirement input bounds".into());
        }
        if self
            .retirement
            .as_ref()
            .is_some_and(|r| r.blocked.is_some())
        {
            return Ok(());
        }
        if let Err(e) = self.advance_retirement(worker, regions, saves).await {
            self.retirement
                .as_mut()
                .ok_or("retirement missing")?
                .blocked = Some(e);
        }
        Ok(())
    }
    async fn advance_retirement(
        &mut self,
        worker: &SimulationWorker,
        regions: &impl GeneratorRegions,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        if self.retirement.as_ref().unwrap().delivery.is_some() {
            self.retirement
                .as_mut()
                .unwrap()
                .delivery
                .as_mut()
                .unwrap()
                .submit(worker);
            return Ok(());
        }
        if let Some(command) = self.retirement.as_ref().unwrap().command.clone() {
            let d = self.delivery(command, delivery::Purpose::Retire)?;
            self.retirement.as_mut().unwrap().delivery = Some(d);
            return Ok(());
        }
        let r = self.retirement.as_mut().unwrap();
        if let Some(change) = r.ticket.inventory.proposal.changes.get(r.next) {
            let before = change
                .before
                .as_ref()
                .ok_or("retirement missing before state")?;
            let placement = match before.place {
                ItemPlace::Contained {
                    container,
                    slot,
                    equipped,
                } => ItemPlacementV2::Contained {
                    container: container.0,
                    slot,
                    pack_slot: before.pack_slot,
                    equipped,
                },
                ItemPlace::World => {
                    let p = r
                        .ticket
                        .positions
                        .get(&before.id)
                        .ok_or("retirement missing accepted world pose")?;
                    ItemPlacementV2::World(bace_content::Position {
                        obj_cell_id: p.cell,
                        position_x: p.origin[0],
                        position_y: p.origin[1],
                        position_z: p.origin[2],
                        rotation_w: p.rotation[3],
                        rotation_x: p.rotation[0],
                        rotation_y: p.rotation[1],
                        rotation_z: p.rotation[2],
                    })
                }
                ItemPlace::Removed => return Err("retirement before state is removed".into()),
            };
            let mut source = if r.ticket.transient.contains(&before.id) {
                regions
                    .transient_source(before.id)
                    .ok_or("missing generated retirement source")?
                    .item
                    .clone()
            } else {
                let stored = self
                    .store
                    .load(before.id.0)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("missing durable retirement baseline")?;
                let durable = match &placement {
                    ItemPlacementV2::World(p) => bace_persistence::DurableItemPlace::World {
                        cell: p.obj_cell_id,
                    },
                    ItemPlacementV2::Contained {
                        container,
                        slot,
                        pack_slot,
                        equipped,
                    } => bace_persistence::DurableItemPlace::Contained {
                        container: *container,
                        slot: *slot,
                        pack_slot: *pack_slot,
                        equipped: *equipped,
                    },
                    _ => return Err("invalid retirement placement".into()),
                };
                let decoded = crate::region_service::world_items::decode(
                    &bace_persistence::LocatedSnapshot {
                        aggregate: stored.clone(),
                        placement: durable,
                        depth: 0,
                    },
                )?;
                if decoded.corpse.is_some() {
                    return Err(
                        "generator lifecycle cannot replace a corpse expiry tombstone".into(),
                    );
                }
                FrozenInventoryItem {
                    corpse: None,
                    construction: decoded.item.construction.clone(),
                    source_destination: decoded.source_destination,
                    entity: decoded.item.previous.previous.entity,
                    placement: Some(decoded.item.previous.previous.placement),
                    enchantments: decoded.item.previous.enchantments,
                    persisted_version: stored.persisted_version,
                }
            };
            if source.entity.object_id != before.id.0
                || source.entity.state.weenie_id != before.template
                || source.entity.mutation_revision > before.revision
            {
                return Err("generator retirement source fence mismatch".into());
            }
            if change.after.place != ItemPlace::Removed {
                if let Some(entries) = r.ticket.enchantments.get(&before.id) {
                    if !r.ticket.registry_revisions.contains_key(&before.id) {
                        return Err("retirement surviving registry revision missing".into());
                    }
                    source.enchantments = entries
                        .iter()
                        .map(crate::enchantment_saves::freeze_enchantment)
                        .collect::<Result<_, _>>()
                        .map_err(|e| e.to_string())?;
                } else if !source.enchantments.is_empty() {
                    return Err("retirement surviving registry capture missing".into());
                }
            }
            // The owner holds every participant. Tombstones intentionally discard
            // qualities; these exact captured inventory values describe the CAS.
            source.entity.mutation_revision = before.revision;
            source.placement = Some(placement);
            set(
                &mut source.entity.state.properties.ints,
                12,
                i32::try_from(before.stack).map_err(|_| "retirement stack overflow")?,
            );
            if let Some(structure) = before.structure {
                set(
                    &mut source.entity.state.properties.ints,
                    92,
                    i32::try_from(structure).map_err(|_| "retirement structure overflow")?,
                );
            } else {
                source.entity.state.properties.ints.retain(|p| p.id != 92);
            }
            r.bytes = r
                .bytes
                .checked_add(
                    bace_storage_codec::ItemSaveV5 {
                        previous: bace_storage_codec::ItemSaveV4 {
                            previous: bace_storage_codec::ItemSaveV3 {
                                previous: bace_storage_codec::ItemSaveV2 {
                                    entity: source.entity.clone(),
                                    placement: source
                                        .placement
                                        .clone()
                                        .ok_or("retirement placement missing")?,
                                },
                                enchantments: source.enchantments.clone(),
                            },
                            construction: source.construction.clone(),
                        },
                        source_destination: source.source_destination,
                    }
                    .encode()
                    .map_err(|e| e.to_string())?
                    .len(),
                )
                .ok_or("retirement source bytes overflow")?;
            if r.bytes > MAX_BYTES {
                return Err("retirement source byte capacity".into());
            }
            r.sources.push(source);
            r.next += 1;
            return Ok(());
        }
        if r.pending.is_none() {
            let operation = format!(
                "generator-retirement:{}:{}",
                self.config.world_epoch, r.ticket.inventory.operation
            );
            r.pending = Some(
                PendingGeneratedRetirement::freeze(
                    r.ticket.clone(),
                    InventoryFreezeInput {
                        operation_id: &operation,
                        proposal: &r.ticket.inventory.proposal,
                        items: &r.sources,
                        other_snapshots: &[],
                        leases: &[],
                        storage_views: &[],
                        admitted_positions: &BTreeMap::new(),
                    },
                    self.config.world_epoch,
                )
                .map_err(|e| e.to_string())?,
            );
        }
        let pending = r.pending.as_mut().unwrap();
        if !r.submitted {
            match pending.submit(saves) {
                Ok(()) => r.submitted = true,
                Err(crate::saves::SaveSubmitError::Full) => {}
                Err(e) => return Err(format!("generator retirement save admission: {e:?}")),
            }
            return Ok(());
        }
        if let Some(resolution) = pending.poll() {
            r.submitted = false;
            match resolution {
                GeneratedRetirementResolution::Committed { receipt, .. } => {
                    r.command = Some(GeneratorAction::CompleteRetirement(receipt))
                }
                GeneratedRetirementResolution::Uncertain { message } => {
                    return Err(format!(
                        "uncertain generator retirement retained: {message}"
                    ));
                }
                GeneratedRetirementResolution::Rejected { failure, .. } => {
                    r.rejected = true;
                    return Err(format!("rejected generator retirement retained: {failure}"));
                }
            }
        }
        Ok(())
    }
}
fn set(values: &mut Vec<bace_content::Property<i32>>, id: u32, value: i32) {
    if let Some(p) = values.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        values.push(bace_content::Property { id, value });
        values.sort_by_key(|p| p.id);
    }
}
