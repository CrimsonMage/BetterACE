//! Bounded trusted host inputs. Client traffic uses authenticated social requests instead.
use bace_gameplay_api::social::{EarnedExperience, SocialError};
use bace_types::EntityId;
#[derive(Clone)]
pub struct SocialControl {
    pub sequence: u64,
    pub action: SocialControlAction,
}
#[derive(Clone)]
pub enum SocialControlAction {
    RestoreAllegiances {
        registry: Box<bace_allegiance::AllegianceRegistry>,
        now_seconds: f64,
    },
    Register {
        presence: bace_social::SocialPresence,
        preferences: bace_social::SocialPreferences,
    },
    Refresh(bace_social::SocialPresence),
    CacheOffline(bace_social::SocialPresence),
    CacheIdentity(bace_gameplay_api::social::SocialIdentity),
    SupplyAllegianceId(u32),
    Award(EarnedExperience),
    RedeemAllegiance(EntityId),
    Commit(Box<crate::AllegianceTicket>),
    Reject(Box<crate::AllegianceTicket>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialControlOutcome {
    pub sequence: u64,
    pub result: Result<Option<u64>, SocialError>,
}
impl SocialControl {
    pub fn valid_bounds(&self) -> bool {
        if self.sequence == 0 {
            return false;
        }
        match &self.action {
            SocialControlAction::RestoreAllegiances {
                registry,
                now_seconds,
            } => {
                registry.nodes().take(1_000_001).count() <= 1_000_000
                    && now_seconds.is_finite()
                    && *now_seconds >= 0.0
            }
            SocialControlAction::Register {
                presence,
                preferences,
            } => presence.identity.name.len() <= 100 && preferences.validate().is_ok(),
            SocialControlAction::Refresh(p) | SocialControlAction::CacheOffline(p) => {
                p.identity.name.len() <= 100
            }
            SocialControlAction::CacheIdentity(identity) => {
                identity.character.0 != 0
                    && identity.account.0 != 0
                    && !identity.name.is_empty()
                    && identity.name.len() <= 100
            }
            SocialControlAction::Commit(t) | SocialControlAction::Reject(t) => {
                t.patch.nodes.len() + t.patch.metadata.len() <= 1024
                    && t.player_changes.len() <= 1024
                    && t.credits.len() <= 1024
            }
            SocialControlAction::Award(r) => r.amount <= i64::MAX as u64,
            _ => true,
        }
    }
}
