//! Correlated staff gag proposal; no mutable player state is duplicated here.
use crate::{ActionContext, social::SocialIdentity};
#[derive(Clone, Debug, PartialEq)]
pub struct StaffGagProposal {
    pub operation: u64,
    pub context: ActionContext,
    pub target: SocialIdentity,
    pub requested_name: String,
    /// None is an explicitly offline target, whose database lease is checked on write.
    pub before_revision: Option<u64>,
    pub enabled: bool,
    pub unix_seconds: f64,
}
