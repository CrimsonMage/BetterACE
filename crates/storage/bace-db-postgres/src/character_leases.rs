//! Bounded indexed ownership reads for multi-character durable transitions.
use crate::{PgStore, StoreError};
use bace_persistence::{CharacterLease, OwnershipState};
impl PgStore {
    /// Missing identities and transitional states remain explicit. This is a
    /// before-image, never authority to skip the transaction's lease locks.
    pub async fn character_leases(&self, ids: &[u32]) -> Result<Vec<CharacterLease>, StoreError> {
        if ids.len() > 1024
            || ids.contains(&0)
            || ids
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != ids.len()
        {
            return Err(StoreError::Invalid("character lease batch bounds"));
        }
        let ids: Vec<i64> = ids.iter().map(|id| i64::from(*id)).collect();
        let rows:Vec<(i64,i64,String)>=sqlx::query_as("SELECT character_id,epoch,state FROM character_ownership WHERE character_id=ANY($1) ORDER BY character_id").bind(&ids).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|(id, epoch, state)| {
                Ok(CharacterLease {
                    character_id: u32::try_from(id)
                        .map_err(|_| StoreError::Invalid("character lease identity"))?,
                    epoch,
                    state: match state.as_str() {
                        "offline" => OwnershipState::Offline,
                        "loading" => OwnershipState::Loading,
                        "online" => OwnershipState::Online,
                        "logging_out" => OwnershipState::LoggingOut,
                        _ => return Err(StoreError::Invalid("character lease state")),
                    },
                })
            })
            .collect()
    }
}
