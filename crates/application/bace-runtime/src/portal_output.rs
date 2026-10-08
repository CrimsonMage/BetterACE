//! Trusted adapter for already projected portal batches. Retain the returned
//! commands on queue pressure; projection/retry must not allocate new stamps.
use crate::{
    game_messages::{GameMessageError, session_batch_command},
    network::NetworkCommand,
};
use bace_replication::{PortalBatch, ReplicationMessage};
use bace_session::SessionKey;
pub struct PortalOutput {
    pub owner: NetworkCommand,
    pub observers: Vec<ReplicationMessage>,
    pub reset_visibility: bool,
}
pub fn portal_output(
    key: SessionKey,
    batch: PortalBatch,
) -> Result<PortalOutput, GameMessageError> {
    let owner = session_batch_command(key, batch.owner)?;
    Ok(PortalOutput {
        owner,
        observers: batch.observers,
        reset_visibility: batch.reset_visibility,
    })
}
