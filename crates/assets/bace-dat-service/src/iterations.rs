use crate::{DddError, DddLimits};
use bace_wire::DddIterationSet;

/// Validates the compact iteration representation without trusting the declared
/// count as proof that the client has every required iteration.
pub(crate) fn present(
    set: &DddIterationSet,
    total: u32,
    limits: DddLimits,
) -> Result<Vec<bool>, DddError> {
    if total > limits.max_iterations
        || set.iterations > limits.max_iterations
        || set.runs.len() > limits.max_iterations as usize * 2
    {
        return Err(DddError::Capacity);
    }
    if set.iterations > total {
        return Err(DddError::NewerClient);
    }
    let mut output = vec![false; total as usize + 1];
    let mut next_length = None;
    let mut count = 0u32;
    for &run in &set.runs {
        if run < 0 {
            let length = run.checked_abs().ok_or(DddError::InvalidIterations)? as u32;
            if length < 2 || next_length.replace(length).is_some() {
                return Err(DddError::InvalidIterations);
            }
            continue;
        }
        if run == 0 {
            return Err(DddError::InvalidIterations);
        }
        let start = run as u32;
        let length = next_length.take().unwrap_or(1);
        let end = start
            .checked_add(length - 1)
            .ok_or(DddError::InvalidIterations)?;
        if end > total {
            return Err(DddError::NewerClient);
        }
        count = count.checked_add(length).ok_or(DddError::Capacity)?;
        if count > set.iterations {
            return Err(DddError::InvalidIterations);
        }
        for index in start..=end {
            if std::mem::replace(&mut output[index as usize], true) {
                return Err(DddError::InvalidIterations);
            }
        }
    }
    if next_length.is_some() || count != set.iterations {
        return Err(DddError::InvalidIterations);
    }
    Ok(output)
}
