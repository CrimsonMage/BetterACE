use crate::RecordMetadata;

#[derive(Clone, Copy, Debug)]
pub struct DddLimits {
    pub max_record_bytes: usize,
    pub max_message_bytes: usize,
    pub max_catalog_records: usize,
    pub max_iterations: u32,
    pub max_pending_records: usize,
    pub max_transfer_bytes: u64,
}
impl Default for DddLimits {
    fn default() -> Self {
        Self {
            max_record_bytes: 256 * 1024 - 64,
            max_message_bytes: 256 * 1024,
            max_catalog_records: 5_000_000,
            max_iterations: 100_000,
            max_pending_records: 16_384,
            max_transfer_bytes: 256 * 1024 * 1024,
        }
    }
}
impl DddLimits {
    pub fn validate(self) -> Result<(), DddError> {
        if self.max_record_bytes == 0
            || self.max_record_bytes > u32::MAX as usize
            || self.max_record_bytes.saturating_add(33) > self.max_message_bytes
            || self.max_catalog_records == 0
            || self.max_iterations == 0
            || self.max_iterations > i32::MAX as u32
            || self.max_pending_records == 0
            || self.max_transfer_bytes == 0
            || self.max_transfer_bytes > u32::MAX as u64
        {
            return Err(DddError::InvalidCatalog);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub enum DddError {
    Wire(bace_wire::WireError),
    Asset(bace_dat::DatError),
    Compression(std::io::Error),
    Capacity,
    InvalidCatalog,
    MissingDatabase,
    InvalidIterations,
    NewerClient,
    Disabled,
    NotFound,
    UnsupportedRequest,
    InvalidState,
    StaleCompletion,
    PreparedRecordMismatch,
}
impl std::fmt::Display for DddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DAT distribution error: {self:?}")
    }
}
impl std::error::Error for DddError {}
impl From<bace_wire::WireError> for DddError {
    fn from(error: bace_wire::WireError) -> Self {
        Self::Wire(error)
    }
}
impl From<bace_dat::DatError> for DddError {
    fn from(error: bace_dat::DatError) -> Self {
        Self::Asset(error)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DddJob {
    pub generation: u64,
    pub record: RecordMetadata,
}
#[derive(Debug, PartialEq, Eq)]
pub enum DddStart {
    UpToDate(Vec<u8>),
    Patch {
        begin: Vec<u8>,
        queued_records: usize,
        transfer_bytes: u32,
    },
}
