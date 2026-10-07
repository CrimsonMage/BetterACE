//! Administrative command handling.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod auth;
mod console;
mod credentials;
mod http;

pub use console::{ControlAction, ControlRequest, HostConsole, HostStatus};
pub use credentials::{HostError, ensure_private_directory, load_operator, provision_operator};
pub use http::{console_router, serve_console};
