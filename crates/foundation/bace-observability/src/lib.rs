//! Logging, metrics and diagnostic interfaces.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod logs;
pub use logs::{ExactLogError, ExactLogErrorKind, LogBatch, LogRecord, LogStore};
