/// Prepared world quest rules. No lookup or I/O occurs while evaluating them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestDefinition {
    pub minimum_delta_seconds: u32,
    pub maximum_solves: i32,
    scale_minimum_delta: bool,
}

impl QuestDefinition {
    /// Pinned ACE exempts names starting with the case-sensitive `ColoArena`
    /// prefix from the configured quest_mindelta_rate.
    pub fn new(name: &str, minimum_delta_seconds: u32, maximum_solves: i32) -> Self {
        Self {
            minimum_delta_seconds,
            maximum_solves,
            scale_minimum_delta: !name.starts_with("ColoArena"),
        }
    }
}

/// Read-only authoritative quest record, not an evolving persistence DTO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestProgress {
    pub last_completed_seconds: u32,
    /// The same signed field is also used by ACE's quest-bit operations.
    pub completions: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestEligibility {
    Ready,
    MissingDefinition,
    MaximumSolves,
    Wait { seconds: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestTimeError {
    InvalidRate,
    DeltaOverflow,
    DeadlineOverflow,
}

/// Equivalent to ACE QuestManager.GetNextSolveTime on valid, non-overflowing
/// inputs. The simulation owner supplies explicit Unix seconds and immutable
/// content/rate inputs; no clock or database is consulted here.
pub fn next_solve(
    definition: Option<&QuestDefinition>,
    progress: Option<&QuestProgress>,
    now_seconds: u32,
    minimum_delta_rate: f64,
) -> Result<QuestEligibility, QuestTimeError> {
    if !minimum_delta_rate.is_finite() || minimum_delta_rate < 0.0 {
        return Err(QuestTimeError::InvalidRate);
    }
    let Some(definition) = definition else {
        return Ok(QuestEligibility::MissingDefinition);
    };
    let Some(progress) = progress else {
        // ACE permits a first solve even if the configured maximum is zero.
        return Ok(QuestEligibility::Ready);
    };
    if definition.maximum_solves > -1 && progress.completions >= definition.maximum_solves {
        return Ok(QuestEligibility::MaximumSolves);
    }
    let delta = if definition.scale_minimum_delta {
        let scaled = f64::from(definition.minimum_delta_seconds) * minimum_delta_rate;
        if scaled >= f64::from(u32::MAX) + 1.0 {
            return Err(QuestTimeError::DeltaOverflow);
        }
        // C#'s valid nonnegative double -> uint conversion truncates fractions.
        scaled as u32
    } else {
        definition.minimum_delta_seconds
    };
    let deadline = progress
        .last_completed_seconds
        .checked_add(delta)
        .ok_or(QuestTimeError::DeadlineOverflow)?;
    if now_seconds >= deadline {
        Ok(QuestEligibility::Ready)
    } else {
        Ok(QuestEligibility::Wait {
            seconds: deadline - now_seconds,
        })
    }
}
