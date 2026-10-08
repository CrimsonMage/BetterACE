/// Frozen binary content in a reviewed runtime namespace. Scalar identity must
/// match the decoded payload before publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeContentCandidate {
    pub namespace: u16,
    pub id: u32,
    pub schema: u16,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct NativePublication {
    pub revision: i64,
    pub expected_manifest_hash: Option<[u8; 32]>,
    pub candidates: Vec<NativeContentCandidate>,
    /// World rows, clothing overrides and explicit removals in the same transaction.
    pub mapped_candidates: Vec<MappedContentCandidate>,
    /// A mixed SQL transaction must never be partially accepted by a consumer.
    pub weenie_candidates: usize,
}

/// A frozen pack record staged for atomic mapped publication. `None` is an
/// explicit tombstone; absence from a batch never removes existing content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedContentCandidate {
    pub namespace: u16,
    pub id: u64,
    pub schema: u16,
    pub bytes: Option<Vec<u8>>,
}
