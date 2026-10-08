//! Explicit keyed draws with ACE's floating and inclusive-integer bounds.
use bace_random::{RandomError, RandomStream};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreasureError {
    Bounds,
    Capacity,
    MissingTemplate(u32),
    MissingTable(u32),
    InvalidTemplate(u32),
    MissingAsset(&'static str, u32),
    MissingSourceTable(String),
    Random(RandomError),
}
impl From<RandomError> for TreasureError {
    fn from(value: RandomError) -> Self {
        Self::Random(value)
    }
}
/// Cloneable cursor permits transactional generation: failed materialization
/// leaves the caller's cursor unchanged, and retry reproduces the same items.
pub trait TreasureRandom: Clone {
    fn unit(&mut self) -> Result<f64, TreasureError>;
    fn inclusive(&mut self, minimum: i32, maximum: i32) -> Result<i32, TreasureError>;
}
impl TreasureRandom for RandomStream {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        Ok((self.next_u64()? >> 11) as f64 / 9007199254740992.0)
    }
    fn inclusive(&mut self, minimum: i32, maximum: i32) -> Result<i32, TreasureError> {
        if maximum < minimum {
            return Err(TreasureError::Bounds);
        }
        Ok((i64::from(minimum)
            + self.below((i64::from(maximum) - i64::from(minimum) + 1) as u64)? as i64)
            as i32)
    }
}
pub(crate) fn unit<R: TreasureRandom>(random: &mut R) -> Result<f64, TreasureError> {
    let value = random.unit()?;
    if !value.is_finite() || !(0.0..1.0).contains(&value) {
        return Err(TreasureError::Bounds);
    }
    Ok(value)
}
pub(crate) fn inclusive<R: TreasureRandom>(
    random: &mut R,
    low: i32,
    high: i32,
) -> Result<i32, TreasureError> {
    if low > high {
        return Err(TreasureError::Bounds);
    }
    let value = random.inclusive(low, high)?;
    if !(low..=high).contains(&value) {
        return Err(TreasureError::Bounds);
    }
    Ok(value)
}
