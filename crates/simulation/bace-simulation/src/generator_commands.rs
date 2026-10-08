//! Bounded trusted adapter inputs and correlated generator service outcomes.
use crate::{GeneratorControl, GeneratorServiceError, InventoryReceipt};
use bace_gameplay_api::{GeneratorIdentity, GeneratorSpawnKey, GeneratorSpawnReceipt};
use bace_inventory::{InventoryContainer, InventoryItem};
use bace_types::EntityId;
use std::sync::Arc;
#[derive(Clone)]
pub enum GeneratorAction {
    /// One nonrecursive trusted cold birth transaction. Every script identity
    /// is validated before the underlying spawn mutates the World.
    AdmitWithScripts {
        sources: Vec<(EntityId, crate::PreparedNpcScriptSource)>,
        action: Box<GeneratorAction>,
    },
    QuiesceRegions,
    ConfirmRegionUnload(crate::RegionUnloadReceipt),
    RetryRegionUnload {
        operation: u64,
    },
    RequestRegion {
        landblock: u16,
        permanent: bool,
    },
    RetryRegion {
        landblock: u16,
        epoch: u64,
    },
    AdmitResidentRegion {
        region: Box<crate::PreparedGeneratorRegion>,
        dungeon: bool,
        keep_alive: u32,
    },
    SupplyId(EntityId),
    DiscardUnusedIdsAfterDrain,
    SupplyRequestIds {
        key: GeneratorSpawnKey,
        expected: usize,
        ids: Vec<EntityId>,
    },
    SetDay(bool),
    AdmitCreature {
        key: GeneratorSpawnKey,
        loadout: Box<crate::PreparedNpcLoadout>,
    },
    AdmitContainedCreature {
        key: GeneratorSpawnKey,
        prepared: Box<crate::PreparedContainedCreature>,
    },
    AdmitContainedForest {
        key: GeneratorSpawnKey,
        prepared: Box<crate::PreparedContainedForest>,
    },
    CompleteRetirement(InventoryReceipt),
    RejectRetirement {
        operation: u64,
    },
    RetryRetirement {
        operation: u64,
    },
    ReserveRequestIds {
        key: GeneratorSpawnKey,
        total: usize,
    },
    RetryRequest(GeneratorSpawnKey),
    RefreshRequest(GeneratorSpawnKey),
    AdmitRegion(Box<crate::PreparedGeneratorRegion>),
    Control {
        identity: GeneratorIdentity,
        control: GeneratorControl,
    },
    SpawnReceipt(GeneratorSpawnReceipt),
    AdmitItems {
        key: GeneratorSpawnKey,
        items: Vec<InventoryItem>,
        containers: Vec<InventoryContainer>,
        shapes: Option<Vec<Arc<bace_physics::CollisionShape>>>,
    },
    AdmitItemTrees {
        key: GeneratorSpawnKey,
        items: Vec<InventoryItem>,
        containers: Vec<InventoryContainer>,
        roots: Vec<EntityId>,
        /// World-root shapes only, in exactly the order of roots.
        shapes: Option<Vec<Arc<bace_physics::CollisionShape>>>,
    },
    AdmitMixedTrees {
        key: GeneratorSpawnKey,
        roots: Vec<crate::PreparedMixedGeneratorRoot>,
    },
    AdmitStockTrees {
        key: GeneratorSpawnKey,
        trees: Vec<crate::PreparedVendorTree>,
    },
    AdmitStock {
        key: GeneratorSpawnKey,
        items: Vec<(EntityId, Arc<bace_content::WeenieV1>)>,
    },
    Acquire {
        receipt: InventoryReceipt,
        transient: Vec<EntityId>,
    },
    RejectAcquisition {
        operation: u64,
    },
}
#[derive(Clone)]
pub struct GeneratorCommand {
    pub correlation: u64,
    pub action: GeneratorAction,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratorCommandOutcome {
    pub correlation: u64,
    pub result: Result<(), GeneratorServiceError>,
    pub admission: Option<GeneratorItemAdmission>,
    /// Exact retained request refreshed without redispatch through the shared FIFO.
    pub request: Option<crate::GeneratorHostRequest>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratorItemAdmission {
    pub key: GeneratorSpawnKey,
    pub roots: Vec<EntityId>,
    pub entities: Vec<EntityId>,
    pub failed_roots: Vec<EntityId>,
}
impl GeneratorCommand {
    pub fn valid_bounds(&self) -> bool {
        if self.correlation == 0 {
            return false;
        }
        self.action.valid_bounds()
    }
}
impl GeneratorAction {
    pub fn spawn_action(&self) -> &Self {
        match self {
            Self::AdmitWithScripts { action, .. } => action,
            other => other,
        }
    }
    pub fn spawn_key(&self) -> Option<GeneratorSpawnKey> {
        match self {
            Self::AdmitCreature { key, .. }
            | Self::AdmitItems { key, .. }
            | Self::AdmitItemTrees { key, .. }
            | Self::AdmitMixedTrees { key, .. } => Some(*key),
            _ => None,
        }
    }
    fn valid_bounds(&self) -> bool {
        match self {
            Self::AdmitWithScripts { sources, action } => {
                action.spawn_key().is_some()
                    && action.valid_bounds()
                    && sources.len() <= 1024
                    && sources.windows(2).all(|p| p[0].0 < p[1].0)
                    && sources
                        .iter()
                        .all(|(id, source)| id.0 != 0 && source.valid_bounds())
                    && sources
                        .iter()
                        .try_fold(0usize, |bytes, (_, source)| {
                            bytes.checked_add(source.properties.retained_bytes()?)
                        })
                        .is_some_and(|bytes| bytes <= 64 * 1024 * 1024)
            }
            GeneratorAction::ConfirmRegionUnload(receipt) => {
                receipt.operation != 0 && receipt.epoch != 0 && receipt.revisions.len() <= 4096
            }
            GeneratorAction::AdmitRegion(region)
            | GeneratorAction::AdmitResidentRegion { region, .. } => {
                region.roots.len() <= 4096
                    && region.containers.len() <= 4096
                    && region.definitions.len() <= 4096
                    && region.templates.len() <= 4096
                    && region.creatures.len() <= 4096
            }
            GeneratorAction::AdmitCreature { loadout, .. } => {
                loadout.items.len() <= 1024
                    && loadout.containers.len() <= 1024
                    && loadout.death_items.len() <= 256
            }
            GeneratorAction::AdmitContainedCreature { prepared, .. } => prepared.valid_bounds(),
            GeneratorAction::AdmitContainedForest { prepared, .. } => prepared.valid_bounds(),
            GeneratorAction::CompleteRetirement(receipt) => receipt.revisions.len() <= 1024,
            GeneratorAction::ReserveRequestIds { total, .. } => *total > 0 && *total <= 1024,
            GeneratorAction::SupplyRequestIds { expected, ids, .. } => {
                *expected > 0
                    && !ids.is_empty()
                    && expected
                        .checked_add(ids.len())
                        .is_some_and(|total| total <= 1024)
            }
            GeneratorAction::AdmitItemTrees {
                items,
                containers,
                roots,
                shapes,
                ..
            } => {
                !items.is_empty()
                    && items.len() <= 1024
                    && containers.len() <= 1024
                    && !roots.is_empty()
                    && roots.len() <= items.len()
                    && shapes.as_ref().is_none_or(|s| s.len() == roots.len())
            }
            GeneratorAction::AdmitItems {
                items,
                containers,
                shapes,
                ..
            } => {
                !items.is_empty()
                    && items.len() <= 1024
                    && containers.len() <= 1024
                    && shapes.as_ref().is_none_or(|s| s.len() == items.len())
            }
            GeneratorAction::AdmitMixedTrees { roots, .. } => {
                !roots.is_empty()
                    && roots.len() <= 1024
                    && roots
                        .iter()
                        .all(crate::PreparedMixedGeneratorRoot::valid_bounds)
                    && roots
                        .iter()
                        .map(crate::PreparedMixedGeneratorRoot::entity_count)
                        .sum::<usize>()
                        <= 1024
                    && roots
                        .iter()
                        .map(crate::PreparedMixedGeneratorRoot::container_count)
                        .sum::<usize>()
                        <= 1024
                    && roots
                        .iter()
                        .map(crate::PreparedMixedGeneratorRoot::enchantment_count)
                        .sum::<usize>()
                        <= 4096
            }
            GeneratorAction::AdmitStockTrees { trees, .. } => {
                !trees.is_empty()
                    && trees.len() <= 1024
                    && trees.iter().all(crate::PreparedVendorTree::valid_bounds)
                    && trees.iter().map(|t| t.items.len() + 1).sum::<usize>() <= 1024
            }
            GeneratorAction::AdmitStock { items, .. } => {
                !items.is_empty()
                    && items.len() <= 1024
                    && items.iter().all(|(_, item)| {
                        item.properties.ints.len() <= 1024
                            && item
                                .properties
                                .strings
                                .iter()
                                .map(|p| p.value.len())
                                .sum::<usize>()
                                <= 65536
                    })
            }
            GeneratorAction::Acquire { receipt, transient } => {
                !transient.is_empty() && transient.len() <= 1024 && receipt.revisions.len() <= 1024
            }
            GeneratorAction::SpawnReceipt(receipt) => match &receipt.result {
                bace_gameplay_api::GeneratorSpawnResult::Completed { members, .. } => {
                    members.len() <= 1024
                }
                _ => true,
            },
            _ => true,
        }
    }
}
