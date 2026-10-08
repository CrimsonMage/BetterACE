//! Retains the exact lineage/player transaction across ambiguous write outcomes.
use crate::saves::{SaveFailure, SaveHandle, SaveSubmitError, SaveTicket, WriteOutcome};
use bace_persistence::{AllegianceCommit, AllegianceOperation, SaveAck};
pub enum AllegianceResolution {
    Committed {
        nodes: Vec<SaveAck>,
        metadata: Vec<SaveAck>,
        players: Vec<SaveAck>,
    },
    Rejected(SaveFailure),
    Uncertain(String),
}
pub struct PendingAllegianceSave {
    operation: AllegianceOperation,
    reply: Option<SaveTicket>,
    uncertain: bool,
    terminal: bool,
}
impl PendingAllegianceSave {
    pub fn new(operation: AllegianceOperation) -> Result<Self, SaveSubmitError> {
        if operation.operation_id.is_empty()
            || operation.operation_id.len() > 128
            || operation.nodes.len() + operation.metadata.len() > 1024
            || operation.players.len() > 1024
            || operation.leases.len() > 1024
            || operation
                .nodes
                .iter()
                .chain(&operation.metadata)
                .any(|w| w.expected_version < 0 || w.expected_version == i64::MAX)
            || operation
                .players
                .iter()
                .any(|w| w.expected_version <= 0 || w.expected_version == i64::MAX)
        {
            return Err(SaveSubmitError::Invalid);
        }
        Ok(Self {
            operation,
            reply: None,
            uncertain: false,
            terminal: false,
        })
    }
    pub fn operation(&self) -> &AllegianceOperation {
        &self.operation
    }
    pub fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        if self.terminal || self.reply.is_some() {
            return Err(SaveSubmitError::Invalid);
        }
        self.reply = Some(saves.try_allegiance(&self.operation)?);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<AllegianceResolution> {
        let report = match self.reply.as_mut()?.try_recv() {
            Ok(report) => report,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.reply = None;
                self.uncertain = true;
                return Some(AllegianceResolution::Uncertain(
                    "allegiance reply closed".into(),
                ));
            }
        };
        self.reply = None;
        let node_acks = |rows: &[bace_persistence::AllegianceWrite]| {
            sorted(
                rows.iter()
                    .map(|w| SaveAck {
                        object_id: w.character,
                        mutation_revision: w.mutation_revision,
                        persisted_version: w.expected_version + 1,
                    })
                    .collect(),
            )
        };
        let nodes = node_acks(&self.operation.nodes);
        let metadata = node_acks(&self.operation.metadata);
        let players = sorted(
            self.operation
                .players
                .iter()
                .map(|w| SaveAck {
                    object_id: w.object_id,
                    mutation_revision: w.mutation_revision,
                    persisted_version: w.expected_version + 1,
                })
                .collect(),
        );
        let result = match report.result {
            Ok(WriteOutcome::Allegiance(AllegianceCommit::AlreadyCommitted)) => Ok(()),
            Ok(WriteOutcome::Allegiance(AllegianceCommit::Committed {
                nodes: n,
                metadata: m,
                players: p,
            })) => {
                if sorted(n) == nodes && sorted(m) == metadata && sorted(p) == players {
                    Ok(())
                } else {
                    Err("allegiance receipt mismatch".into())
                }
            }
            Err(
                error @ SaveFailure::Storage {
                    uncertain: false, ..
                },
            ) if !self.uncertain => {
                self.terminal = true;
                return Some(AllegianceResolution::Rejected(error));
            }
            Err(error) => Err(error.to_string()),
            Ok(_) => Err("allegiance receipt mismatch".into()),
        };
        Some(match result {
            Ok(()) => {
                self.terminal = true;
                AllegianceResolution::Committed {
                    nodes,
                    metadata,
                    players,
                }
            }
            Err(error) => {
                self.uncertain = true;
                AllegianceResolution::Uncertain(error)
            }
        })
    }
}
fn sorted(mut rows: Vec<SaveAck>) -> Vec<SaveAck> {
    rows.sort_by_key(|r| r.object_id);
    rows
}
