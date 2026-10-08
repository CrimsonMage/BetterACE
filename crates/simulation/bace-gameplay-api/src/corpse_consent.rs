//! Authenticated requests for ACE's per-player corpse-loot consent list.
//! The death owner checks online identity, consent option, expiry and durable
//! corpse access. Names and action sequence remain untrusted proposals here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorpseConsentRequest {
    Clear,
    Display,
    RemoveFrom(String),
    Add(String),
    Remove(String),
}
