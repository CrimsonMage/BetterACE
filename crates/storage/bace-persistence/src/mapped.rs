/// Immutable mapped-content manifest. Files MUST be durable before acceptance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedGeneration {
    pub manifest_hash: [u8; 32],
    pub parent_hash: Option<[u8; 32]>,
    pub base_hash: [u8; 32],
    pub accepted_revision: i64,
    pub manifest_bytes: Vec<u8>,
}
