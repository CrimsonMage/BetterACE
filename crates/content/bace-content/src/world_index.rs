//! Frozen per-landblock lookup record. IDs refer to the original world rows.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandblockIndexV1 {
    pub landblock: u16,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub instance_ids: Vec<u32>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub encounter_ids: Vec<u32>,
}

/// Lookup of authored links owned by one parent; preserves source row IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceLinkIndexV1 {
    pub parent_guid: u32,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub children: Vec<InstanceLinkTargetV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceLinkTargetV1 {
    pub link_id: u32,
    pub child_guid: u32,
}
