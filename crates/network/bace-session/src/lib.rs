//! Session lifecycle kernel; account verification and world entry are external.
//! This crate does not implement a playable stock-client login flow.
mod lifecycle;
mod registry;
pub use lifecycle::{SessionError, SessionLifecycle, SessionState};
pub use registry::{AdmissionError, RegisteredSession, SessionKey, SessionRegistry};
mod login;
pub use login::{LoginRejection, PasswordLogin, validate_password_login};
mod dispatch;
pub use dispatch::{DispatchError, DispatchedProgression, decode_progression};
mod accounts;
pub use accounts::{AccountAdmission, AccountCancellation, AccountSessionError, AccountSessions};
