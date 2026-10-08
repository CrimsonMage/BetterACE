//! One exact frozen placement operation survives queue pressure and uncertain
//! commits. No caller may turn an ambiguous outcome into a rollback.
use crate::saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome};
use bace_persistence::{OperationOutcome, PlacementOperation, SaveAck};
#[derive(Debug)]
pub enum PlacementResolution {
    Committed(Vec<SaveAck>),
    Rejected(SaveFailure),
    Uncertain(String),
}
enum FrozenPlacement {
    Plain(PlacementOperation),
    World(bace_persistence::WorldPlacementOperation),
    Constructed(bace_persistence::ConstructedCreaturePromotionOperation),
    Allegiance(bace_persistence::AllegiancePlacementOperation),
}
pub struct PendingPlacementSave {
    operation: FrozenPlacement,
    reply: Option<SaveTicket>,
    uncertain: bool,
    terminal: bool,
}
impl PendingPlacementSave {
    pub fn new(operation: PlacementOperation) -> Result<Self, SaveSubmitError> {
        validate_operation(&operation)?;
        Ok(Self {
            operation: FrozenPlacement::Plain(operation),
            reply: None,
            uncertain: false,
            terminal: false,
        })
    }
    pub fn new_world(
        operation: bace_persistence::WorldPlacementOperation,
    ) -> Result<Self, SaveSubmitError> {
        validate_operation(&operation.inventory)?;
        if operation.world_epoch == 0 || operation.world_epoch > i64::MAX as u64 {
            return Err(SaveSubmitError::Invalid);
        }
        Ok(Self {
            operation: FrozenPlacement::World(operation),
            reply: None,
            uncertain: false,
            terminal: false,
        })
    }
    pub fn new_constructed(
        operation: bace_persistence::ConstructedCreaturePromotionOperation,
    ) -> Result<Self, SaveSubmitError> {
        validate_operation(&operation.inventory)?;
        if operation.world_epoch == 0
            || operation.world_epoch > i64::MAX as u64
            || operation.creature_roots.is_empty()
            || operation.creature_roots.len() > 128
        {
            return Err(SaveSubmitError::Invalid);
        }
        Ok(Self {
            operation: FrozenPlacement::Constructed(operation),
            reply: None,
            uncertain: false,
            terminal: false,
        })
    }
    pub fn new_allegiance(
        operation: bace_persistence::AllegiancePlacementOperation,
    ) -> Result<Self, SaveSubmitError> {
        validate_operation(&operation.placement)?;
        if operation.world_epoch == 0
            || operation.allegiance.operation_id != operation.placement.operation_id
            || !operation.allegiance.players.is_empty()
        {
            return Err(SaveSubmitError::Invalid);
        }
        Ok(Self {
            operation: FrozenPlacement::Allegiance(operation),
            reply: None,
            uncertain: false,
            terminal: false,
        })
    }
    pub fn operation(&self) -> &PlacementOperation {
        match &self.operation {
            FrozenPlacement::Plain(op) => op,
            FrozenPlacement::World(op) => &op.inventory,
            FrozenPlacement::Constructed(op) => &op.inventory,
            FrozenPlacement::Allegiance(op) => &op.placement,
        }
    }
    pub fn allegiance_operation(&self) -> Option<&bace_persistence::AllegiancePlacementOperation> {
        match &self.operation {
            FrozenPlacement::Allegiance(op) => Some(op),
            _ => None,
        }
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.terminal || self.reply.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.reply = Some(match &self.operation {
            FrozenPlacement::Plain(op) => saves.try_placement(op)?,
            FrozenPlacement::World(op) => saves.try_world_placement(op)?,
            FrozenPlacement::Constructed(op) => saves.try_constructed_creature_promotion(op)?,
            FrozenPlacement::Allegiance(op) => saves.try_allegiance_placement(op)?,
        });
        Ok(())
    }
    pub fn poll(&mut self) -> Option<PlacementResolution> {
        let report = match self.reply.as_mut()?.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.reply = None;
                self.uncertain = true;
                return Some(PlacementResolution::Uncertain(
                    "placement reply closed".into(),
                ));
            }
        };
        self.reply = None;
        let mut expected: Vec<_> = self
            .operation()
            .snapshots
            .iter()
            .map(|s| SaveAck {
                object_id: s.object_id,
                mutation_revision: s.mutation_revision,
                persisted_version: s.expected_version + 1,
            })
            .collect();
        expected.sort_by_key(|a| a.object_id);
        match report.result {
            Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)) => {
                self.terminal = true;
                Some(PlacementResolution::Committed(expected))
            }
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(mut actual))) => {
                actual.sort_by_key(|a| a.object_id);
                if actual == expected {
                    self.terminal = true;
                    Some(PlacementResolution::Committed(actual))
                } else {
                    self.uncertain = true;
                    Some(PlacementResolution::Uncertain(
                        "placement receipt mismatch".into(),
                    ))
                }
            }
            Err(
                failure @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                Some(PlacementResolution::Rejected(failure))
            }
            Err(error) => {
                self.uncertain = true;
                Some(PlacementResolution::Uncertain(error.to_string()))
            }
            Ok(_) => {
                self.uncertain = true;
                Some(PlacementResolution::Uncertain(
                    "unexpected placement reply".into(),
                ))
            }
        }
    }
}

fn validate_operation(operation: &PlacementOperation) -> Result<(), SaveSubmitError> {
    if operation.operation_id.is_empty()
        || operation.operation_id.len() > 128
        || operation.snapshots.is_empty()
        || operation.snapshots.len() > 1024
        || operation.participants.len() > 1024
        || operation.leases.len() > 1024
        || operation
            .snapshots
            .iter()
            .any(|s| s.expected_version < 0 || s.expected_version == i64::MAX)
    {
        return Err(SaveSubmitError::Invalid);
    }
    Ok(())
}
