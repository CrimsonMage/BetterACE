//! Scalar legal turn intent, separate from a client's reported orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnIntent {
    axis: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnError {
    NonFinite,
    OutOfRange,
}
/// Server-generated controller correlation, never an inbound pose/heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TurnControl {
    pub owner: u64,
    pub sequence: u64,
}
impl TurnIntent {
    pub fn new(axis: f32) -> Result<Self, TurnError> {
        if !axis.is_finite() {
            Err(TurnError::NonFinite)
        } else if !(-1.0..=1.0).contains(&axis) {
            Err(TurnError::OutOfRange)
        } else {
            Ok(Self { axis })
        }
    }
    pub fn axis(self) -> f32 {
        self.axis
    }
}
/// Signed shortest target delta in radians. Neither input is client authority.
pub fn heading_delta(current: f32, target: f32) -> Result<f32, TurnError> {
    if !current.is_finite() || !target.is_finite() {
        return Err(TurnError::NonFinite);
    }
    Ok(
        (target - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI,
    )
}
