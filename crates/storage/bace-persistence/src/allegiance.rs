//! Separate allegiance aggregate identity space; player XP and lineage changes
//! share one durable operation and generation-fenced character leases.
use crate::{CharacterLease, SaveAck, SaveSnapshot};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllegianceWrite {
    pub character: u32,
    pub mutation_revision: u64,
    pub expected_version: i64,
    pub bytes: Option<Vec<u8>>,
}
#[derive(Clone, Debug)]
pub struct AllegianceOperation {
    pub operation_id: String,
    pub nodes: Vec<AllegianceWrite>,
    pub metadata: Vec<AllegianceWrite>,
    pub players: Vec<SaveSnapshot>,
    pub leases: Vec<CharacterLease>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredAllegiance {
    pub character: u32,
    pub mutation_revision: u64,
    pub persisted_version: i64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllegianceCommit {
    Committed {
        nodes: Vec<SaveAck>,
        metadata: Vec<SaveAck>,
        players: Vec<SaveAck>,
    },
    AlreadyCommitted,
}

/// One durable outcome for a corpse/NPC inventory stage and shared XP lineage.
#[derive(Clone, Debug)]
pub struct AllegiancePlacementOperation {
    pub placement: crate::PlacementOperation,
    pub allegiance: AllegianceOperation,
    pub world_epoch: u64,
    pub workflow: Option<crate::NpcWorkflowUpdate>,
}
