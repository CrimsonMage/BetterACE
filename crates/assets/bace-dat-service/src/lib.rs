//! Bounded legacy DAT distribution planning and asset-worker preparation.
mod catalog;
mod iterations;
mod preparation;
mod session;
mod types;
pub use catalog::{DatabaseMetadata, DddCatalog, RecordMetadata};
pub use preparation::{
    PreparedRecord, prepare_archive_record, prepare_archive_record_uncompressed, prepare_record,
};
pub use session::DddSession;
pub use types::{DddError, DddJob, DddLimits, DddStart};
