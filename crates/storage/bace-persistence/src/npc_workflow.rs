//! One source-defined NPC stage commits its effects and continuation together.
#[derive(Clone, Debug)]
pub struct NpcWorkflowUpdate {
    pub invocation: [u8; 16],
    pub world_epoch: u64,
    pub expected_version: i64,
    pub checkpoint: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct NpcStageOperation {
    pub inventory: crate::PlacementOperation,
    pub workflow: NpcWorkflowUpdate,
}
#[derive(Clone, Debug)]
pub struct StoredNpcWorkflow {
    pub invocation: [u8; 16],
    pub version: i64,
    pub checkpoint: Vec<u8>,
}
