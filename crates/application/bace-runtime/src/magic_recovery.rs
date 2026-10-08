//! Frozen cast recovery conversion; the caller supplies trusted explicit clocks.
use bace_magic::{CastRecovery, MagicSchool};
use bace_storage_codec::{FrozenCombatRecoveryV1, SaveCodecError};
pub fn freeze_cast_recovery(value: CastRecovery) -> Result<FrozenCombatRecoveryV1, SaveCodecError> {
    value
        .validate()
        .map_err(|_| SaveCodecError::Invalid("cast recovery"))?;
    let frozen = FrozenCombatRecoveryV1 {
        schema_version: 1,
        revision: value.revision,
        minimum_remaining: value.minimum_remaining,
        streak_remaining: value.streak_remaining,
        last_success_school: value.last_success_school.map_or(0, |s| s as u8),
        last_success_age: value.last_success_age,
    };
    frozen.validate()?;
    Ok(frozen)
}
pub fn prepare_cast_recovery(
    value: FrozenCombatRecoveryV1,
) -> Result<CastRecovery, SaveCodecError> {
    value.validate()?;
    Ok(CastRecovery {
        revision: value.revision,
        minimum_remaining: value.minimum_remaining,
        streak_remaining: value.streak_remaining,
        last_success_age: value.last_success_age,
        last_success_school: match value.last_success_school {
            0 => None,
            1 => Some(MagicSchool::War),
            2 => Some(MagicSchool::Life),
            3 => Some(MagicSchool::Creature),
            4 => Some(MagicSchool::Item),
            5 => Some(MagicSchool::Void),
            _ => return Err(SaveCodecError::Invalid("cast school")),
        },
    })
}
