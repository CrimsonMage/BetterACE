//! Exact committed-revision gate for complete player ownership transfer.
use bace_gameplay_api::CharacterBinding;
use bace_types::EntityId;
#[derive(Clone, Debug)]
pub struct PlayerDetachRequest {
    pub correlation: u64,
    pub binding: CharacterBinding,
    pub expected_revision: u64,
    pub capture_final: bool,
    pub expected_items: Vec<(EntityId, u64)>,
}
impl PlayerDetachRequest {
    /// Bound retained input before crossing either owner queue. Full identity
    /// and revision agreement is checked against the accepted owner snapshot.
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && self.expected_revision != 0
            && self.expected_items.len() <= 1023
            && self
                .expected_items
                .iter()
                .all(|(id, revision)| id.0 != 0 && *id != self.binding.actor && *revision != 0)
    }
}
pub struct DetachedPlayer {
    pub snapshot: std::sync::Arc<crate::PlayerReadSnapshot>,
    pub state: crate::OwnedPlayerState,
    pub actor: bace_entity::Actor,
    pub items: Vec<bace_inventory::InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
}
pub struct PlayerDetachOutcome {
    pub correlation: u64,
    pub binding: CharacterBinding,
    pub result: Result<DetachedPlayer, crate::CharacterRegistrationError>,
}
