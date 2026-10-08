//! Cast provenance is explicit: trusted server sources never impersonate sessions.
use crate::{ActionContext, CastChange, CastRejection};
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastOrigin {
    Player(ActionContext),
    Staff {
        actor: EntityId,
        event: u64,
    },
    Monster {
        actor: EntityId,
        event: u64,
    },
    Emote {
        actor: EntityId,
        event: u64,
        instant: bool,
    },
    PhysicalProc {
        actor: EntityId,
        event: u64,
    },
    ItemProc {
        actor: EntityId,
        item: EntityId,
        event: u64,
    },
}
impl CastOrigin {
    pub fn actor(self) -> EntityId {
        match self {
            Self::Player(c) => c.actor,
            Self::Staff { actor, .. }
            | Self::Monster { actor, .. }
            | Self::Emote { actor, .. }
            | Self::ItemProc { actor, .. }
            | Self::PhysicalProc { actor, .. } => actor,
        }
    }
    pub fn server_event(self) -> Option<(u8, u64)> {
        match self {
            Self::Player(_) => None,
            Self::Staff { event, .. } => Some((4, event)),
            Self::Monster { event, .. } => Some((1, event)),
            Self::Emote { event, .. } => Some((2, event)),
            Self::ItemProc { event, .. } => Some((3, event)),
            Self::PhysicalProc { event, .. } => Some((5, event)),
        }
    }
    pub fn instant(self) -> bool {
        matches!(
            self,
            Self::Staff { .. }
                | Self::Emote { instant: true, .. }
                | Self::ItemProc { .. }
                | Self::PhysicalProc { .. }
        )
    }
    pub fn uses_resources(self) -> bool {
        !matches!(
            self,
            Self::Staff { .. }
                | Self::ItemProc { .. }
                | Self::PhysicalProc { .. }
                | Self::Emote { instant: true, .. }
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerCastOutcome {
    pub origin: CastOrigin,
    pub result: Result<CastChange, CastRejection>,
}
