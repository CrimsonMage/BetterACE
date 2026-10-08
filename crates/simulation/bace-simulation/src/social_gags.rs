//! Single simulation-owned gag state and exact pending command proposals.
use bace_types::EntityId;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GagRecovery {
    pub state: bace_social::GagState,
    /// A source heartbeat delayed by output/valuable holds; no offline wall-time debit.
    pub pending_interval: Option<f64>,
}
impl GagRecovery {
    pub fn validate(self) -> Result<(), &'static str> {
        self.state.validate().map_err(|_| "gag properties")?;
        if self
            .pending_interval
            .is_some_and(|n| !n.is_finite() || n <= 0.)
        {
            return Err("gag heartbeat interval");
        }
        Ok(())
    }
}
pub(crate) struct PendingGag {
    pub(crate) proposal: bace_gameplay_api::staff_gags::StaffGagProposal,
    pub(crate) change: Option<bace_social::GagChange>,
    pub(crate) issuer_name: String,
}
#[derive(Default)]
pub(crate) struct SocialGags {
    pub(crate) states: BTreeMap<EntityId, GagRecovery>,
    pub(crate) pending: BTreeMap<u64, PendingGag>,
    pub(crate) scratch: Vec<EntityId>,
}
