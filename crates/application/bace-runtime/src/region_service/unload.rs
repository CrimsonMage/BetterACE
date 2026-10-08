//! Captured world roots remain held while their exact checkpoint is persisted.
use super::*;
use crate::region_unload_saves::{
    PendingRegionUnloadSave, RegionItemSource, RegionUnloadResolution,
};
pub(super) struct Unloading {
    ticket: bace_simulation::RegionUnloadTicket,
    sources: Vec<RegionItemSource>,
    next: usize,
    bytes: usize,
    save: Option<PendingRegionUnloadSave>,
    submitted: bool,
    receipt: Option<bace_simulation::RegionUnloadReceipt>,
    acknowledged: bool,
}
impl Unloading {
    pub(super) fn landblock(&self) -> u16 {
        self.ticket.landblock
    }
}
impl RegionService {
    pub(super) fn finish_unload(&mut self, landblock: u16, epoch: u64) {
        if self.unload.as_ref().is_some_and(|u| {
            u.ticket.landblock == landblock && u.ticket.epoch == epoch && u.acknowledged
        }) {
            let u = self.unload.take().expect("matched unload");
            for item in &u.ticket.items {
                self.sources.remove(&item.item.id.0);
            }
        }
    }
    pub(super) async fn poll_unload(
        &mut self,
        worker: &SimulationWorker,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        if self.unload.is_none() {
            if let Ok(ticket) = worker.region_unload_proposals().try_recv() {
                self.unload = Some(Unloading {
                    ticket,
                    sources: vec![],
                    next: 0,
                    bytes: 0,
                    save: None,
                    submitted: false,
                    receipt: None,
                    acknowledged: false,
                });
            } else {
                return Ok(());
            }
        }
        let block = self.unload.as_ref().unwrap().ticket.landblock;
        if self
            .blocked
            .get(&block)
            .is_some_and(|reason| blocks_unload(reason))
        {
            return Ok(());
        }
        if let Err(error) = self.advance_unload(worker, saves).await {
            self.blocked.insert(block, error);
        }
        Ok(())
    }
    async fn advance_unload(
        &mut self,
        worker: &SimulationWorker,
        saves: &SaveHandle,
    ) -> Result<(), String> {
        let u = self.unload.as_mut().unwrap();
        if u.acknowledged {
            return Ok(());
        }
        if let Some(receipt) = &u.receipt {
            let command = bace_simulation::Command::Generator(bace_simulation::GeneratorCommand {
                correlation: u.ticket.operation,
                action: bace_simulation::GeneratorAction::ConfirmRegionUnload(receipt.clone()),
            });
            if worker.input().try_submit(command).is_ok() {
                // The owner received its exact durable receipt. It retains the
                // saved capture through any subsequent physical teardown delay.
                u.acknowledged = true;
            }
            return Ok(());
        }
        if let Some(captured) = u.ticket.items.get(u.next) {
            let mut source = if captured.transient {
                let known = self
                    .sources
                    .get(&captured.item.id.0)
                    .ok_or("missing retained transient source at region unload")?;
                RegionItemSource {
                    item: known.item.clone(),
                    corpse: known.corpse.clone(),
                }
            } else {
                let stored = self
                    .store
                    .load(captured.item.id.0)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("missing durable region source")?;
                let placement = match captured.item.place {
                    bace_inventory::ItemPlace::World => captured
                        .position
                        .map(|p| bace_persistence::DurableItemPlace::World { cell: p.cell }),
                    bace_inventory::ItemPlace::Contained {
                        container,
                        slot,
                        equipped,
                    } => Some(bace_persistence::DurableItemPlace::Contained {
                        container: container.0,
                        slot,
                        pack_slot: captured.item.pack_slot,
                        equipped,
                    }),
                    _ => None,
                }
                .ok_or("invalid captured region placement")?;
                let decoded = world_items::decode(&LocatedSnapshot {
                    aggregate: stored.clone(),
                    placement,
                    depth: 0,
                })?;
                RegionItemSource {
                    item: crate::game_inventory::FrozenInventoryItem {
                        corpse: decoded.corpse.clone().map(Box::new),
                        construction: decoded.item.construction.clone(),
                        source_destination: decoded.source_destination,
                        entity: decoded.item.previous.previous.entity,
                        placement: Some(decoded.item.previous.previous.placement),
                        persisted_version: stored.persisted_version,
                        enchantments: decoded.item.previous.enchantments,
                    },
                    corpse: decoded.corpse,
                }
            };
            // Fresh generated sources have no fabricated accepted world pose.
            // This exact held owner capture supplies the live placement.
            if captured.transient {
                source.item.placement = Some(match captured.item.place {
                    bace_inventory::ItemPlace::Contained {
                        container,
                        slot,
                        equipped,
                    } => bace_storage_codec::ItemPlacementV2::Contained {
                        container: container.0,
                        slot,
                        pack_slot: captured.item.pack_slot,
                        equipped,
                    },
                    bace_inventory::ItemPlace::World => {
                        let p = captured
                            .position
                            .ok_or("transient world item capture has no accepted pose")?;
                        bace_storage_codec::ItemPlacementV2::World(bace_content::Position {
                            obj_cell_id: p.cell,
                            position_x: p.origin[0],
                            position_y: p.origin[1],
                            position_z: p.origin[2],
                            rotation_w: p.rotation[0],
                            rotation_x: p.rotation[1],
                            rotation_y: p.rotation[2],
                            rotation_z: p.rotation[3],
                        })
                    }
                    _ => return Err("transient region capture is removed".into()),
                });
            }
            let bytes = world_items::source_size(&source)?;
            let total = u
                .bytes
                .checked_add(bytes)
                .ok_or("region unload source size overflow")?;
            if total > 64 * 1024 * 1024 {
                return Err("region unload source byte capacity".into());
            }
            u.bytes = total;
            u.sources.push(source);
            u.next += 1;
            return Ok(());
        }
        if u.save.is_none() {
            u.save = Some(crate::region_unload_saves::freeze_region_unload(
                self.config.world_epoch,
                &u.ticket,
                &u.sources,
            )?);
        }
        let save = u.save.as_mut().unwrap();
        if !u.submitted {
            match save.submit(saves) {
                Ok(()) => u.submitted = true,
                Err(crate::saves::SaveSubmitError::Full) => {}
                Err(e) => return Err(format!("region unload submission: {e:?}")),
            }
            return Ok(());
        }
        if let Some(result) = save.poll() {
            u.submitted = false;
            match result {
                RegionUnloadResolution::Progress => {}
                RegionUnloadResolution::Committed { receipt, .. } => u.receipt = Some(receipt),
                RegionUnloadResolution::Uncertain(error) => {
                    return Err(format!("uncertain region unload: {error}"));
                }
                RegionUnloadResolution::Blocked(error) => {
                    return Err(format!("blocked region unload: {error}"));
                }
            }
        }
        Ok(())
    }
}
fn blocks_unload(reason: &str) -> bool {
    !reason.starts_with("resident refresh")
}
#[cfg(test)]
mod refresh_unload_tests {
    use super::blocks_unload;
    #[test]
    fn refresh_failure_cannot_gate_checkpoint_retry() {
        assert!(!blocks_unload("resident refresh rejected: Stale"));
        assert!(!blocks_unload(
            "resident refresh preparation: invalid asset"
        ));
        assert!(blocks_unload(
            "uncertain region unload: database unavailable"
        ));
    }
}
