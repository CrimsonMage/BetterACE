//! Correlated durable portal receipts; admission is distinct from materialization.
use crate::{Kernel, PortalServiceReceipt};
use bace_interactions::PortalError;
use bace_types::EntityId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortalResolution {
    Commit(PortalServiceReceipt),
    Reject {
        operation: u64,
        actor: EntityId,
    },
    DestinationReady {
        operation: u64,
        actor: EntityId,
        epoch: u16,
    },
    ClientReady {
        context: bace_gameplay_api::ActionContext,
        operation: u64,
        epoch: u16,
    },
}
impl PortalResolution {
    pub fn operation(&self) -> u64 {
        match self {
            Self::Commit(r) => r.operation,
            Self::Reject { operation, .. }
            | Self::DestinationReady { operation, .. }
            | Self::ClientReady { operation, .. } => *operation,
        }
    }
    pub fn actor(&self) -> EntityId {
        match self {
            Self::Commit(r) => r.actor,
            Self::Reject { actor, .. } | Self::DestinationReady { actor, .. } => *actor,
            Self::ClientReady { context, .. } => context.actor,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PortalResolutionCommand {
    pub correlation: u64,
    pub resolution: PortalResolution,
}
impl PortalResolutionCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && (self.resolution.operation() != 0
                || matches!(self.resolution, PortalResolution::ClientReady { .. }))
            && self.resolution.actor().0 != 0
            && match &self.resolution {
                PortalResolution::Commit(r) => (1..=9).contains(&r.revisions.len()),
                PortalResolution::Reject { .. }
                | PortalResolution::DestinationReady { .. }
                | PortalResolution::ClientReady { .. } => true,
            }
    }
}
#[derive(Clone, Debug)]
pub struct PortalResolutionOutcome {
    pub correlation: u64,
    pub resolution: PortalResolution,
    pub result: Result<(), PortalError>,
}
impl Kernel {
    pub fn apply_portal_resolution(
        &mut self,
        command: PortalResolutionCommand,
    ) -> PortalResolutionOutcome {
        let result = if !command.valid_bounds() {
            Err(PortalError::Invalid)
        } else {
            match &command.resolution {
                PortalResolution::Commit(receipt) => self.confirm_portal_committed(receipt),
                PortalResolution::DestinationReady {
                    operation,
                    actor,
                    epoch,
                } => self.mark_portal_destination_ready(*actor, *operation, *epoch),
                PortalResolution::ClientReady {
                    context,
                    operation,
                    epoch,
                } => self.acknowledge_portal_ready(*context, *operation, *epoch),
                PortalResolution::Reject { operation, actor } => {
                    if self
                        .pending_portal_proposal(*operation)
                        .is_none_or(|p| p.actor != *actor)
                    {
                        Err(PortalError::Conflict)
                    } else {
                        self.reject_portal_proposal(*operation)
                    }
                }
            }
        };
        PortalResolutionOutcome {
            correlation: command.correlation,
            resolution: command.resolution,
            result,
        }
    }
}
