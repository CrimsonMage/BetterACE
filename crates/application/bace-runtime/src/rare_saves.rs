//! Pure translation of a frozen rare proposal into a new player save. The
//! caller commits this WITH corpse/items/rewards, then confirms the world ticket.
use bace_gameplay_api::{CharacterRareState, RareDecision};
use bace_random::RandomRoot;
use bace_storage_codec::{PlayerSaveV6, RareStateV1, SaveCodecError};

pub fn initial_rare_state(
    root: &RandomRoot,
    character: u32,
) -> Result<CharacterRareState, SaveCodecError> {
    Ok(CharacterRareState {
        character,
        random_identity: root
            .character_identity(character)
            .map_err(|_| SaveCodecError::Invalid("rare bootstrap identity"))?,
        key_version: root.key_version(),
        attempt_ordinal: 0,
        timer_ordinal: 0,
        next_realtime_at: None,
        last_effective_time: 0,
    })
}
pub fn rare_state_from_save(state: RareStateV1) -> CharacterRareState {
    CharacterRareState {
        character: state.character,
        random_identity: state.random_identity,
        key_version: state.key_version,
        attempt_ordinal: state.attempt_ordinal,
        timer_ordinal: state.timer_ordinal,
        next_realtime_at: state.next_realtime_at,
        last_effective_time: state.last_effective_time,
    }
}
pub fn rare_state_to_save(state: CharacterRareState) -> RareStateV1 {
    RareStateV1 {
        character: state.character,
        random_identity: state.random_identity,
        key_version: state.key_version,
        attempt_ordinal: state.attempt_ordinal,
        timer_ordinal: state.timer_ordinal,
        next_realtime_at: state.next_realtime_at,
        last_effective_time: state.last_effective_time,
    }
}
pub fn freeze_rare_decision(
    current: &PlayerSaveV6,
    decision: &RareDecision,
    mutation_revision: u64,
) -> Result<PlayerSaveV6, SaveCodecError> {
    current.validate()?;
    if current.player.entity.object_id != decision.character
        || decision.previous.character != decision.character
        || decision.next.character != decision.character
    {
        return Err(SaveCodecError::Invalid("rare proposal character mismatch"));
    }
    if let Some(previous) = current.rares {
        if rare_state_from_save(previous) != decision.previous {
            return Err(SaveCodecError::Invalid("stale rare proposal"));
        }
    } else if decision.previous.attempt_ordinal != 0
        || decision.previous.timer_ordinal != 0
        || decision.previous.next_realtime_at.is_some()
        || decision.previous.last_effective_time != 0
    {
        return Err(SaveCodecError::Invalid(
            "rare bootstrap must begin from an empty stream",
        ));
    }
    if !decision.eligible {
        if decision.next != decision.previous
            || decision.award.is_some()
            || decision.standard_success
            || decision.realtime_success
        {
            return Err(SaveCodecError::Invalid("ineligible rare mutation"));
        }
        return Ok(current.clone());
    }
    if decision.previous.random_identity != decision.next.random_identity
        || decision.previous.key_version != decision.next.key_version
        || decision.previous.attempt_ordinal.checked_add(1) != Some(decision.next.attempt_ordinal)
        || decision.next.timer_ordinal < decision.previous.timer_ordinal
        || decision.next.last_effective_time < decision.previous.last_effective_time
        || decision.award.is_some() != (decision.standard_success || decision.realtime_success)
    {
        return Err(SaveCodecError::Invalid(
            "rare proposal does not preserve stream progression",
        ));
    }
    if mutation_revision <= current.player.entity.mutation_revision {
        return Err(SaveCodecError::Invalid(
            "rare mutation needs the authoritative new aggregate revision",
        ));
    }
    let mut next = current.clone();
    next.rares = Some(rare_state_to_save(decision.next));
    next.player.entity.mutation_revision = mutation_revision;
    next.validate()?;
    Ok(next)
}
