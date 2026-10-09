use super::*;

#[derive(Clone, Debug)]
pub struct HousingWrite {
    pub operation_id: String,
    pub owner: CharacterLease,
    pub house: HouseSaveV1,
    pub expected_version: i64,
}

pub(super) enum GameplayWrite {
    AllegiancePlacement(Box<bace_persistence::AllegiancePlacementOperation>),
    Allegiance(bace_persistence::AllegianceOperation),
    Inventory(InventoryOperation),
    NpcStage(Box<NpcStageOperation>),
    Placement(PlacementOperation),
    WorldPlacement(WorldPlacementOperation),
    VendorStock(bace_persistence::VendorStockOperation),
    ConstructedCreaturePromotion(bace_persistence::ConstructedCreaturePromotionOperation),
    HousingLifecycle(HousingOperation),
    Housing(Box<HousingWrite>),
}

impl SaveHandle {
    pub fn try_allegiance_placement(
        &self,
        operation: &bace_persistence::AllegiancePlacementOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        let patch = &operation.allegiance;
        if operation.world_epoch == 0
            || patch.operation_id != operation.placement.operation_id
            || !patch.players.is_empty()
            || patch.nodes.len() + patch.metadata.len() > 1024
            || patch.leases.len() > 1024
        {
            return Err(SaveSubmitError::Invalid);
        }
        let base = placement_bytes_inner(&operation.placement, operation.workflow.is_some())?;
        let bytes = patch
            .nodes
            .iter()
            .chain(&patch.metadata)
            .try_fold(base as usize, |sum, row| {
                sum.checked_add(row.bytes.as_ref().map_or(0, Vec::len) + 32)
            })
            .and_then(|n| n.checked_add(patch.leases.len() * 32 + patch.operation_id.len()))
            .and_then(|n| {
                n.checked_add(
                    operation
                        .workflow
                        .as_ref()
                        .map_or(0, |w| w.checkpoint.len() + 64),
                )
            })
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || {
            GameplayWrite::AllegiancePlacement(Box::new(operation.clone()))
        })
    }

    pub fn try_allegiance(
        &self,
        operation: &bace_persistence::AllegianceOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if operation.operation_id.is_empty()
            || operation.operation_id.len() > 128
            || operation.nodes.len() + operation.metadata.len() > 1024
            || operation.nodes.is_empty()
                && operation.metadata.is_empty()
                && operation.players.is_empty()
            || operation.players.len() > 1024
            || operation.leases.len() > 1024
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = operation
            .nodes
            .iter()
            .chain(&operation.metadata)
            .try_fold(0usize, |sum, w| {
                sum.checked_add(w.bytes.as_ref().map_or(0, Vec::len) + 32)
            })
            .and_then(|sum| {
                operation
                    .players
                    .iter()
                    .try_fold(sum, |sum, p| sum.checked_add(p.bytes.len() + 32))
            })
            .and_then(|sum| {
                sum.checked_add(operation.leases.len() * 32 + operation.operation_id.len())
            })
            .and_then(|sum| u32::try_from(sum).ok())
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || GameplayWrite::Allegiance(operation.clone()))
    }

    pub fn try_npc_stage(
        &self,
        operation: &NpcStageOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        let checkpoint = bace_storage_codec::NpcWorkflowSaveV3::decode_or_migrate(
            &operation.workflow.checkpoint,
        )
        .map_err(|_| SaveSubmitError::Invalid)?;
        if checkpoint.invocation != operation.workflow.invocation
            || operation.workflow.expected_version < 0
            || !operation
                .inventory
                .participants
                .contains(&checkpoint.source)
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = placement_bytes_inner(&operation.inventory, true)?
            .checked_add(
                u32::try_from(operation.workflow.checkpoint.len())
                    .map_err(|_| SaveSubmitError::Invalid)?,
            )
            .and_then(|n| n.checked_add(32))
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || {
            GameplayWrite::NpcStage(Box::new(operation.clone()))
        })
    }

    /// Borrowed input stays with the reserving owner on overload and uncertainty.
    pub fn try_inventory(
        &self,
        operation: &InventoryOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if operation.operation_id.is_empty()
            || operation.operation_id.len() > 128
            || operation.snapshots.is_empty()
            || operation.snapshots.len() > 1024
            || operation.leases.len() > 1024
            || operation.transfers.len() > 1024
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = operation
            .snapshots
            .iter()
            .try_fold(0_usize, |sum, s| {
                if s.bytes.is_empty() || s.bytes.len() > 17 * 1024 * 1024 {
                    None
                } else {
                    sum.checked_add(s.bytes.len())
                }
            })
            .and_then(|n| {
                n.checked_add(
                    operation.operation_id.len()
                        + operation.leases.len() * 32
                        + operation.transfers.len() * 64,
                )
            })
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || GameplayWrite::Inventory(operation.clone()))
    }

    /// Routine owned-aggregate writes use the reserved age-ordered routine lane,
    /// never critical capacity. Caller retains dirty snapshots and original age.
    pub fn try_owned_batch(
        &self,
        batch: &OwnedSaveBatch,
        dirty_since: Instant,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if dirty_since > Instant::now()
            || batch.snapshots.is_empty()
            || batch.snapshots.len() > 1024
            || batch.participants.is_empty()
            || batch.participants.len() > 1024
            || batch.leases.len() > 1024
        {
            return Err(SaveSubmitError::Invalid);
        }
        let mut ids = std::collections::BTreeSet::new();
        if batch
            .participants
            .iter()
            .any(|id| *id == 0 || !ids.insert(*id))
            || batch
                .snapshots
                .iter()
                .any(|s| s.expected_version <= 0 || !ids.contains(&s.object_id))
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = batch
            .snapshots
            .iter()
            .try_fold(0usize, |sum, s| {
                if s.bytes.is_empty() || s.bytes.len() > 17 * 1024 * 1024 {
                    None
                } else {
                    sum.checked_add(s.bytes.len())
                }
            })
            .and_then(|n| n.checked_add(batch.participants.len() * 4 + batch.leases.len() * 32))
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(SaveSubmitError::Invalid)?;
        if bytes == 0 || bytes > self.routine_limit || bytes > 64 * 1024 * 1024 {
            return Err(SaveSubmitError::Invalid);
        }
        if *self.close.borrow() {
            return Err(SaveSubmitError::Closed);
        }
        let slot = self.routine.try_reserve().map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => SaveSubmitError::Full,
            mpsc::error::TrySendError::Closed(_) => SaveSubmitError::Closed,
        })?;
        let permit = self
            .routine_budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| SaveSubmitError::Full)?;
        let (reply, ticket) = oneshot::channel();
        slot.send(Request {
            gameplay: None,
            owned_batch: Some(batch.clone()),
            offline: None,
            owner: None,
            snapshots: Vec::new(),
            operation_id: None,
            dirty_since: Some(dirty_since),
            reply,
            _budget: permit,
            order: 0,
        });
        Ok(ticket)
    }

    pub fn try_placement(
        &self,
        operation: &PlacementOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        let bytes = placement_bytes(operation)?;
        self.submit_gameplay(bytes, || GameplayWrite::Placement(operation.clone()))
    }
    /// Preserve the exact world epoch on retry, including uncertain commits.
    pub fn try_world_placement(
        &self,
        operation: &WorldPlacementOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if operation.world_epoch == 0 || operation.world_epoch > i64::MAX as u64 {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = placement_bytes(&operation.inventory)?
            .checked_add(8)
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || GameplayWrite::WorldPlacement(operation.clone()))
    }
    /// Marker CAS and player/item/vendor snapshots share one bounded critical
    /// slot. A failed admission leaves the caller's exact request untouched.
    pub fn try_vendor_stock(
        &self,
        operation: &bace_persistence::VendorStockOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        let marker = bace_storage_codec::VendorStockSaveV1::decode(&operation.marker.bytes)
            .map_err(|_| SaveSubmitError::Invalid)?;
        if operation.world_epoch == 0
            || operation.world_epoch > i64::MAX as u64
            || operation.vendor_expected_version <= 0
            || operation.marker.marker_object_id == 0
            || operation.marker.expected_version < 0
            || operation.marker.bytes.is_empty()
            || operation.marker.bytes.len() > 17 * 1024 * 1024
            || marker.marker_object_id != operation.marker.marker_object_id
            || marker.stock_revision <= operation.marker.expected_stock_revision
            || !operation
                .inventory
                .participants
                .contains(&marker.vendor_object_id)
            || !operation
                .inventory
                .participants
                .contains(&marker.marker_object_id)
        {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = placement_bytes_inner(&operation.inventory, true)?
            .checked_add(8 + 8 + 8 + 8 + 8)
            .and_then(|n| n.checked_add(operation.marker.bytes.len() as u32))
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || GameplayWrite::VendorStock(operation.clone()))
    }
    /// Fresh generated Creature/Cow graphs require their own durable graph
    /// check before the simulation owner can adopt the acquisition receipt.
    pub fn try_constructed_creature_promotion(
        &self,
        operation: &bace_persistence::ConstructedCreaturePromotionOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if operation.world_epoch == 0
            || operation.world_epoch > i64::MAX as u64
            || operation.creature_roots.is_empty()
            || operation.creature_roots.len() > 128
        {
            return Err(SaveSubmitError::Invalid);
        }
        let root_bytes = u32::try_from(operation.creature_roots.len())
            .ok()
            .and_then(|len| len.checked_mul(4))
            .ok_or(SaveSubmitError::Invalid)?;
        let bytes = placement_bytes(&operation.inventory)?
            .checked_add(8 + root_bytes)
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || {
            GameplayWrite::ConstructedCreaturePromotion(operation.clone())
        })
    }

    pub fn try_housing_operation(
        &self,
        operation: &HousingOperation,
    ) -> Result<SaveTicket, SaveSubmitError> {
        let bytes = placement_bytes(&operation.inventory)?
            .checked_add(64)
            .ok_or(SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || GameplayWrite::HousingLifecycle(operation.clone()))
    }

    pub fn try_housing(&self, operation: &HousingWrite) -> Result<SaveTicket, SaveSubmitError> {
        if operation.operation_id.is_empty() || operation.operation_id.len() > 128 {
            return Err(SaveSubmitError::Invalid);
        }
        let bytes = operation
            .house
            .encode()
            .map_err(|_| SaveSubmitError::Invalid)?;
        let bytes = u32::try_from(bytes.len() + operation.operation_id.len() + 64)
            .map_err(|_| SaveSubmitError::Invalid)?;
        self.submit_gameplay(bytes, || {
            GameplayWrite::Housing(Box::new(operation.clone()))
        })
    }

    fn submit_gameplay(
        &self,
        bytes: u32,
        make: impl FnOnce() -> GameplayWrite,
    ) -> Result<SaveTicket, SaveSubmitError> {
        if bytes == 0 || bytes > self.valuable_limit {
            return Err(SaveSubmitError::Invalid);
        }
        if *self.close.borrow() {
            return Err(SaveSubmitError::Closed);
        }
        let slot = self.valuable.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => SaveSubmitError::Full,
            mpsc::error::TrySendError::Closed(_) => SaveSubmitError::Closed,
        })?;
        let permit = self
            .valuable_budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| SaveSubmitError::Full)?;
        let (reply, ticket) = oneshot::channel();
        slot.send(Request {
            gameplay: Some(make()),
            owned_batch: None,
            offline: None,
            owner: None,
            snapshots: Vec::new(),
            operation_id: None,
            dirty_since: None,
            reply,
            _budget: permit,
            order: 0,
        });
        Ok(ticket)
    }
}

fn placement_bytes(operation: &PlacementOperation) -> Result<u32, SaveSubmitError> {
    placement_bytes_inner(operation, false)
}
fn placement_bytes_inner(
    operation: &PlacementOperation,
    allow_empty: bool,
) -> Result<u32, SaveSubmitError> {
    if operation.operation_id.is_empty()
        || operation.operation_id.len() > 128
        || (!allow_empty && operation.snapshots.is_empty())
        || operation.snapshots.len() > 1024
        || operation.participants.is_empty()
        || operation.participants.len() > 1024
        || operation.leases.len() > 1024
        || operation.changes.len() > 1024
        || operation.storage_views.len() > 64
    {
        return Err(SaveSubmitError::Invalid);
    }
    operation
        .snapshots
        .iter()
        .try_fold(0usize, |sum, s| {
            if s.bytes.is_empty() || s.bytes.len() > 17 * 1024 * 1024 {
                None
            } else {
                sum.checked_add(s.bytes.len())
            }
        })
        .and_then(|n| {
            n.checked_add(
                operation.operation_id.len()
                    + operation.participants.len() * 4
                    + operation.leases.len() * 32
                    + operation.changes.len() * 64
                    + operation.storage_views.len() * 24,
            )
        })
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(SaveSubmitError::Invalid)
}
