//! World metadata follows exact durable placement. The region cache is a frozen
//! source cache, never a second mutable inventory or physical owner.
use super::*;
use bace_inventory::ItemPlace;
use bace_storage_codec::ItemPlacementV2;
impl GameRuntime {
    pub(super) fn register_inventory_world_sources(&mut self) -> Result<bool, String> {
        let Some(pending) = self.inventory.pending.as_mut() else {
            return Ok(true);
        };
        let Some(completion) = pending.completion.as_ref().filter(|c| c.committed) else {
            return Ok(true);
        };
        if pending.sources_registered {
            return Ok(true);
        }
        let Some(world) = self.world.as_mut() else {
            return Ok(false);
        };
        let mut rows = BTreeMap::new();
        for change in &completion.work.operation.ticket.proposal.changes {
            if change.after.place == ItemPlace::Removed {
                continue;
            }
            let row = completion
                .snapshots
                .iter()
                .find(|r| r.object_id == change.after.id.0)
                .ok_or("committed placement snapshot missing")?;
            let (saved, corpse) = crate::game_inventory::decode_inventory_item(&row.bytes, None)
                .map_err(|e| e.to_string())?;
            rows.insert(change.after.id.0, (saved, row.expected_version, corpse));
        }
        let mut acquired: Vec<_> = completion
            .work
            .operation
            .ticket
            .proposal
            .changes
            .iter()
            .filter(|c| c.after.place == ItemPlace::Removed)
            .map(|c| c.after.id)
            .collect();
        for (&id, (saved, version, corpse)) in &rows {
            let mut place = &saved.placement;
            let mut landblock = None;
            let mut owned = false;
            for _ in 0..1024 {
                match place {
                    ItemPlacementV2::World(p) => {
                        landblock = Some((p.obj_cell_id >> 16) as u16);
                        break;
                    }
                    ItemPlacementV2::Contained { container, .. } => {
                        if *container == pending.binding.actor.0
                            || self
                                .online_saves
                                .inventory_baseline(pending.binding.actor.0, *container)
                                .is_some()
                        {
                            owned = true;
                            break;
                        }
                        place = if let Some((parent, _, _)) = rows.get(container) {
                            &parent.placement
                        } else {
                            completion
                                .work
                                .external
                                .iter()
                                .find(|p| p.entity.object_id == *container)
                                .and_then(|p| p.placement.as_ref())
                                .ok_or("inventory world parent metadata missing")?
                        };
                    }
                    _ => return Err("removed world source retained".into()),
                }
            }
            if owned {
                acquired.push(EntityId(id));
                continue;
            }
            let block = landblock.ok_or("inventory world ancestry capacity")?;
            let source = crate::region_unload_saves::RegionItemSource {
                item: crate::game_inventory::FrozenInventoryItem {
                    corpse: corpse.clone().map(Box::new),
                    construction: saved.construction.clone(),
                    source_destination: saved.source_destination,
                    entity: saved.entity.clone(),
                    placement: Some(saved.placement.clone()),
                    persisted_version: *version,
                    enchantments: saved.enchantments.clone(),
                },
                corpse: corpse.clone(),
            };
            if world.regions.record_source(block, source).is_err() {
                return Ok(false);
            }
        }
        world.regions.forget_inventory_sources(&acquired)?;
        pending.sources_registered = true;
        Ok(true)
    }
}
