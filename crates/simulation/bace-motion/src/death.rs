//! Immutable source StopCompletely→Motion_Dead preparation. World owns execution.
use crate::PreparedMotionChain;
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct PreparedDeathMotion {
    pub stop: Arc<PreparedMotionChain>,
    pub dead: Arc<PreparedMotionChain>,
}
