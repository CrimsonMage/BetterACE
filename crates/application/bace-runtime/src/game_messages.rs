//! Convert completed lifecycle/projection results into trusted network commands.
//! Queue admission is not delivery. The caller retains results on backpressure.
use crate::{
    game_login::{CommittedCharacter, GameLoginError, GameLoginService},
    network::NetworkCommand,
};
use bace_replication::SessionBatch;
use bace_session::SessionKey;
use bace_wire::{CharacterList, CharacterListEntry, CharacterReply, WireError};

pub async fn roster_command(
    service: &GameLoginService,
    key: SessionKey,
) -> Result<NetworkCommand, GameMessageError> {
    roster_command_with_chat(service, key, false).await
}
pub(crate) async fn roster_command_with_chat(
    service: &GameLoginService,
    key: SessionKey,
    use_turbine_chat: bool,
) -> Result<NetworkCommand, GameMessageError> {
    let characters = service
        .roster(key)
        .await?
        .into_iter()
        .map(|p| CharacterListEntry {
            object_id: p.object_id,
            name: p.name,
            seconds_disabled: 0,
        })
        .collect();
    let bytes = CharacterList {
        characters,
        slot_count: u32::from(service.maximum_slots()),
        account: service.account_name(key)?.to_owned(),
        use_turbine_chat,
        has_throne_of_destiny: true,
    }
    .encode(service.maximum_slots().into())?;
    Ok(NetworkCommand::Send {
        key,
        queue: 9,
        bytes,
    })
}
pub fn creation_command(committed: &CommittedCharacter) -> Result<NetworkCommand, WireError> {
    Ok(NetworkCommand::Send {
        key: committed.key,
        queue: 9,
        bytes: CharacterReply::Created {
            object_id: committed.object_id,
            name: committed.name.clone(),
        }
        .encode()?,
    })
}
pub fn session_batch_command(
    key: SessionKey,
    batch: SessionBatch,
) -> Result<NetworkCommand, GameMessageError> {
    if key.generation != batch.binding.session.0 {
        return Err(GameMessageError::StaleSession);
    }
    Ok(NetworkCommand::SendOrderedBatch {
        key,
        messages: batch
            .messages
            .into_iter()
            .map(|m| (m.queue, m.bytes))
            .collect(),
    })
}
#[derive(Debug, thiserror::Error)]
pub enum GameMessageError {
    #[error(transparent)]
    Lifecycle(#[from] GameLoginError),
    #[error(transparent)]
    Wire(#[from] WireError),
    #[error("stale replication generation")]
    StaleSession,
}
