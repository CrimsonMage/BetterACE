//! ACE DDDHandler and Structure/{CAllIterationList,PTaggedIterationList,
//! CMostlyConsecutiveIntSet}. Counts and run lengths are bounded before allocation.
use crate::envelope::expect_opcode;
use crate::opcode::GameMessageOpcode;
use crate::{Reader, WireError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DddIterationSet {
    pub dat_file_type: i32,
    pub dat_file_id: i32,
    pub iterations: u32,
    /// Signed run representation retained without expanding a potentially huge set.
    pub runs: Vec<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DddInterrogationResponse {
    pub client_language: u32,
    pub with_keys: Vec<DddIterationSet>,
    /// Official handler ignores the second list and flags; no semantics are inferred.
    pub trailing_bytes: usize,
}
impl DddInterrogationResponse {
    pub fn decode(
        bytes: &[u8],
        max_lists: usize,
        max_iterations: u32,
        max_runs: usize,
    ) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::DDD_InterrogationResponse)?;
        let client_language = reader.u32()?;
        let count = reader.u32()?;
        if count > i32::MAX as u32 || count as usize > max_lists {
            return Err(WireError::LimitExceeded);
        }
        if count as usize > reader.remaining() / 12 {
            return Err(WireError::Truncated);
        }
        let mut with_keys = Vec::with_capacity(count as usize);
        let mut total_runs = 0usize;
        for _ in 0..count {
            let dat_file_type = reader.u32()? as i32;
            let dat_file_id = reader.u32()? as i32;
            let iterations = reader.u32()?;
            if iterations > i32::MAX as u32 || iterations > max_iterations {
                return Err(WireError::LimitExceeded);
            }
            let mut runs = Vec::new();
            let mut covered = 0u32;
            while covered < iterations {
                if total_runs >= max_runs {
                    return Err(WireError::LimitExceeded);
                }
                let run = reader.u32()? as i32;
                let increment = if run < 0 {
                    run.checked_abs().ok_or(WireError::InvalidEncoding)? as u32 - 1
                } else {
                    1
                };
                covered = covered
                    .checked_add(increment)
                    .ok_or(WireError::InvalidLength)?;
                if covered > iterations {
                    return Err(WireError::InvalidEncoding);
                }
                runs.push(run);
                total_runs += 1;
            }
            with_keys.push(DddIterationSet {
                dat_file_type,
                dat_file_id,
                iterations,
                runs,
            });
        }
        Ok(Self {
            client_language,
            with_keys,
            trailing_bytes: reader.remaining(),
        })
    }
}
