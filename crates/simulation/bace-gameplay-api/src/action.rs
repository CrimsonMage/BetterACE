use bace_types::{AccountId, EntityId};

/// Server-assigned connection identity. This is not a client-selected wire ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

/// Correlation supplied by the authenticated session adapter. The simulation
/// owner must validate the active account/actor binding before applying a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionContext {
    pub session: SessionId,
    pub account: AccountId,
    pub actor: EntityId,
    pub sequence: u32,
}

/// Authoritative association installed by the owning lifecycle adapter. SessionId
/// must identify a connection generation, not a recycled legacy u16 wire slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterBinding {
    pub session: SessionId,
    pub account: AccountId,
    pub actor: EntityId,
}

/// A domain result, not a durable acknowledgment or an encoded network message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionResult<T, E> {
    pub context: ActionContext,
    pub result: Result<T, E>,
}
