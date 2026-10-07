//! Logging, metrics and diagnostic interfaces.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod logs;
pub use logs::{LogBatch, LogRecord, LogStore};
