//! Persistence-side monotonicity checks for the frozen recovery supplement.
use crate::{CombatRecoverySaveV1, SaveCodecError};
pub fn validate_recovery_transition(
    before: Option<CombatRecoverySaveV1>,
    after: Option<CombatRecoverySaveV1>,
) -> Result<(), SaveCodecError> {
    if let Some(after) = after {
        after.validate()?;
    }
    let Some(before) = before else {
        return Ok(());
    };
    before.validate()?;
    let after = after.ok_or(SaveCodecError::Invalid(
        "cannot erase cast recovery history",
    ))?;
    if after.state.revision < before.state.revision
        || after.captured_unix_millis < before.captured_unix_millis
    {
        return Err(SaveCodecError::Invalid(
            "cast recovery revision or clock rewind",
        ));
    }
    if after.state.revision == before.state.revision {
        let elapsed = (after.captured_unix_millis - before.captured_unix_millis) as f64 / 1000.0;
        // UTC bounds the maximum legal aging. Fixed-step simulation may have
        // advanced less during a scheduler stall; retaining a longer lock must
        // not reject otherwise valid player saves. A same-revision timer cannot
        // reset, advance faster than elapsed time, or rewrite its spell school.
        let tolerance = 0.001000001;
        let countdown = |old: f64, new: f64| {
            new <= old + tolerance && new + tolerance >= (old - elapsed).max(0.0)
        };
        if after.state.last_success_school != before.state.last_success_school
            || !countdown(
                before.state.minimum_remaining,
                after.state.minimum_remaining,
            )
            || !countdown(before.state.streak_remaining, after.state.streak_remaining)
            || before.state.last_success_school != 0
                && (after.state.last_success_age + tolerance < before.state.last_success_age
                    || after.state.last_success_age
                        > (before.state.last_success_age + elapsed).min(5.0) + tolerance)
        {
            return Err(SaveCodecError::Invalid(
                "cast recovery changed without a new revision",
            ));
        }
    }
    Ok(())
}
