/// Native binary content candidate. Decoding and semantic validation belong to content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentCandidate {
    pub wcid: u32,
    pub class_name: String,
    pub weenie_type: i32,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Publication {
    pub revision: i64,
    pub candidates: Vec<ContentCandidate>,
}
/// Full aggregate snapshot; mutation revision is not the database CAS version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveSnapshot {
    pub object_id: u32,
    pub mutation_revision: u64,
    /// Zero means the object does not yet exist. Each committed write increments this.
    pub expected_version: i64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveAck {
    pub object_id: u32,
    pub mutation_revision: u64,
    pub persisted_version: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredAggregate {
    pub object_id: u32,
    pub persisted_version: i64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationOutcome {
    Committed(Vec<SaveAck>),
    AlreadyCommitted,
}
