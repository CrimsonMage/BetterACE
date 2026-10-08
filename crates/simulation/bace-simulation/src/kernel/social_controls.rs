//! Trusted control completion is correlated and never inferred from queue admission.
use super::Kernel;
use crate::{SocialControl, SocialControlAction as A, SocialControlOutcome};
impl Kernel {
    pub fn peek_social_control_outcome(&self) -> Option<&SocialControlOutcome> {
        self.social.controls.front()
    }
    pub fn take_social_control_outcome(&mut self) -> Option<SocialControlOutcome> {
        self.social.controls.pop_front()
    }
    pub fn peek_allegiance_proposal(&self) -> Option<&crate::AllegianceTicket> {
        if self.allegiances.submitted || self.allegiances.pve_operation.is_some() {
            None
        } else {
            self.allegiances.pending.as_ref()
        }
    }
    pub(in crate::kernel) fn apply_social_control(&mut self, command: SocialControl) {
        let sequence = command.sequence;
        let result = if !command.valid_bounds() {
            Err(bace_gameplay_api::social::SocialError::Invalid)
        } else {
            match command.action {
                A::RestoreAllegiances {
                    registry,
                    now_seconds,
                } => self
                    .register_allegiances(*registry, now_seconds)
                    .map(|()| None),
                A::Register {
                    presence,
                    preferences,
                } => self
                    .register_social_presence(presence, preferences)
                    .map(|()| None),
                A::CacheOffline(p) => self
                    .social
                    .directory
                    .cache_offline_identity(p)
                    .map(|()| None),
                A::CacheIdentity(identity) => self
                    .social
                    .directory
                    .cache_identity(identity)
                    .map(|()| None),
                A::Refresh(p) => self.refresh_social_presence(p).map(|()| None),
                A::SupplyAllegianceId(id) => self.supply_allegiance_id(id).map(|()| None),
                A::Award(reward) => self
                    .prepare_shared_experience(reward)
                    .map(|ticket| Some(ticket.operation)),
                A::RedeemAllegiance(actor) => self
                    .prepare_allegiance_login_experience(actor)
                    .map(|ticket| ticket.map(|t| t.operation)),
                A::Commit(ticket) => self
                    .confirm_allegiance_committed(&ticket)
                    .map(|()| Some(ticket.operation)),
                A::Reject(ticket) => self
                    .reject_allegiance_proposal(&ticket)
                    .map(|()| Some(ticket.operation)),
            }
        };
        self.social
            .controls
            .push_back(SocialControlOutcome { sequence, result });
    }
}
