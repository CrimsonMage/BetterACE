//! Exact relational uniqueness conflicts for definite player-creation rollback.
use crate::StoreError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerCreationConflict {
    Name,
    Slot,
}
impl StoreError {
    pub fn player_creation_conflict(&self) -> Option<PlayerCreationConflict> {
        let Self::Sql(error) = self else {
            return None;
        };
        let db = error.as_database_error()?;
        if db.code().as_deref() != Some("23505") {
            return None;
        }
        match db.constraint() {
            Some("players_canonical_name_key") => Some(PlayerCreationConflict::Name),
            Some("players_account_id_slot_key") => Some(PlayerCreationConflict::Slot),
            _ => None,
        }
    }
}
