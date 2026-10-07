//! Bounded password hashing and account repository contracts for fresh accounts.
//! Blocking hash work belongs on bounded adapter workers, never world ticks.
mod account;
mod attempts;
mod error;
mod login;
mod password;
mod repository;
pub use account::{AccessLevel, AccountName, AccountRecord, NewAccount};
pub use attempts::{AttemptError, LoginAttempts};
pub use error::AuthError;
pub use login::{LoginError, PasswordCredentials, PasswordWorker, authenticate};
pub use password::{PasswordHashRecord, PasswordService};
pub use repository::{AccountRepository, CreateAccountOutcome};
