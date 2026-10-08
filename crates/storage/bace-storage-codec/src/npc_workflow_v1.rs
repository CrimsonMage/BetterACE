//! Frozen NPC execution checkpoint V1. Durable stages preserve the ACE timeline.
use crate::{CodecLimits, SaveCodecError, npc_values_v1::*};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcScheduledRowV1 {
    pub inline: bool,
    pub depth: u16,
    pub set: u32,
    pub action: u32,
    pub due: f64,
    pub order: u64,
    pub context: NpcContextV1,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcPendingRowV1 {
    pub ticket: u64,
    pub row: NpcScheduledRowV1,
}
pub use crate::npc_effects_v1::*;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcInvocationSaveV1 {
    pub operation: u64,
    pub event_id: [u8; 16],
    pub key_version: u32,
    pub random_position: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcWorkflowSaveV1 {
    pub invocation: [u8; 16],
    pub source: u32,
    pub program_hash: [u8; 32],
    pub content_generation: [u8; 32],
    pub logical_now: f64,
    pub event_id: [u8; 16],
    pub key_version: u32,
    pub random_position: u64,
    pub active_operation: u64,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub invocations: Vec<NpcInvocationSaveV1>,
    pub stage: u64,
    pub completed: bool,
    pub next_order: u64,
    pub remaining_instructions: u32,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub scheduled: Vec<NpcScheduledRowV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub pending: Vec<NpcPendingRowV1>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub detached: Vec<u64>,
    #[serde(deserialize_with = "crate::gameplay_save::bounded")]
    pub effects: Vec<NpcPendingEffectV1>,
}
impl NpcWorkflowSaveV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        crate::npc_workflow_validation::validate(self)
    }
    pub fn encode(&self) -> Result<Vec<u8>, SaveCodecError> {
        self.validate()?;
        Ok(crate::encode(120, 1, self, limits())?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveCodecError> {
        let value: Self = crate::decode(bytes, 120, 1, limits())?;
        value.validate()?;
        Ok(value)
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_payload_bytes: 2 * 1024 * 1024,
    }
}
