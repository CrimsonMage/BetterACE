//! Join the source's held gear rows once and advance caches only on a committed
//! NPC journal receipt. Uncertainty retains the original binary beforeimages.
use super::*;
type Frozen = crate::npc_persistence::FrozenNpcSourceInventory;
pub(super) fn prepare(
    saved: &mut BTreeMap<EntityId, Arc<Frozen>>,
    regions: Option<&crate::region_service::RegionService>,
    definitions: &BTreeMap<EntityId, crate::npc_sources::PreparedNpcRegistration>,
    snapshot: Option<&bace_simulation::NpcSourceInventorySnapshot>,
    epoch: u64,
) -> Result<Option<Arc<Frozen>>, String> {
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    if let Some(existing) = saved.get(&snapshot.source) {
        return if existing.ticket == snapshot.ticket {
            Ok(Some(existing.clone()))
        } else {
            Err("NPC source frozen stage still retained".into())
        };
    }
    let regions = regions.ok_or("NPC source world metadata unavailable")?;
    let items = snapshot
        .items
        .iter()
        .map(|i| {
            regions
                .npc_item_source(i.item.id)
                .map(|s| s.item.clone())
                .ok_or("NPC source gear metadata missing")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut definition = definitions
        .get(&snapshot.source)
        .ok_or("NPC source definition missing")?
        .clone();
    if definition.baseline.is_none() && snapshot.root_item.is_some() {
        let source = &regions
            .npc_item_source(snapshot.source)
            .ok_or("NPC source item baseline missing")?
            .item;
        if source.corpse.is_some() {
            return Err("NPC source corpse unsupported".into());
        }
        let item = bace_storage_codec::ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: bace_storage_codec::ItemSaveV2 {
                    entity: source.entity.clone(),
                    placement: source
                        .placement
                        .clone()
                        .ok_or("NPC source item placement missing")?,
                },
                enchantments: source.enchantments.clone(),
            },
            construction: source.construction.clone(),
        };
        let item = bace_storage_codec::ItemSaveV5 {
            previous: item,
            source_destination: source.source_destination,
        };
        definition.baseline = Some(bace_persistence::StoredAggregate {
            object_id: snapshot.source.0,
            persisted_version: source.persisted_version,
            bytes: item.encode().map_err(|e| e.to_string())?,
        });
    }
    let prepared = Arc::new(crate::npc_persistence::freeze_source_inventory(
        snapshot,
        &items,
        epoch,
        &definition,
    )?);
    let bytes = saved
        .values()
        .flat_map(|v| v.snapshots())
        .chain(prepared.snapshots())
        .try_fold(0usize, |n, s| n.checked_add(s.bytes.len()))
        .ok_or("NPC gear retained byte overflow")?;
    if bytes > 64 * 1024 * 1024 {
        return Err("NPC gear retained byte capacity".into());
    }
    saved.insert(snapshot.source, prepared.clone());
    Ok(Some(prepared))
}
impl GameRuntime {
    pub(super) fn accept_npc_source_inventory(
        &mut self,
        event: &NpcCoordinatorEvent,
    ) -> Result<(), String> {
        use crate::npc_persistence::{
            NpcCheckpointResolution as C, NpcHandInResolution as H, NpcStageResolution as S,
        };
        let (source, committed) = match event {
            NpcCoordinatorEvent::Durable { source, resolution } => match resolution.as_ref() {
                S::Committed { .. } => (*source, true),
                S::Rejected { .. } => (*source, false),
                _ => return Ok(()),
            },
            NpcCoordinatorEvent::HandIn { source, resolution } => match resolution.as_ref() {
                H::Committed { .. } => (*source, true),
                H::Rejected { .. } => (*source, false),
                _ => return Ok(()),
            },
            NpcCoordinatorEvent::Checkpoint { source, resolution } => match resolution {
                C::Committed => (*source, true),
                C::Rejected(_) => (*source, false),
                _ => return Ok(()),
            },
            _ => return Ok(()),
        };
        if let Some(frozen) = self.npc.source_inventory.get(&source) {
            if committed {
                let adopt_root = frozen.root_snapshot().is_some()
                    && frozen.root_is_item()
                    && !frozen.root_is_static_shop();
                if !frozen.snapshots().is_empty() || adopt_root {
                    let world = self
                        .world
                        .as_mut()
                        .ok_or("NPC source world receipt owner missing")?;
                    world.regions.adopt_npc_item_snapshots(frozen.snapshots())?;
                    if adopt_root {
                        world
                            .regions
                            .adopt_npc_item_snapshots(std::slice::from_ref(
                                frozen.root_snapshot().expect("matched NPC root receipt"),
                            ))?;
                    }
                }
                if let Some(root) = frozen.root_snapshot() {
                    self.npc
                        .definitions
                        .get_mut(&source)
                        .ok_or("NPC source root baseline owner missing")?
                        .baseline = Some(bace_persistence::StoredAggregate {
                        object_id: root.object_id,
                        persisted_version: root.expected_version + 1,
                        bytes: root.bytes.clone(),
                    });
                }
            }
            self.npc.source_inventory.remove(&source);
        }
        Ok(())
    }
}
