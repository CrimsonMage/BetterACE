//! Frozen cast recovery supplement. capture_unix_seconds belongs to the outer
//! player save; elapsed offline time is supplied by the trusted lifecycle adapter.
use crate::SaveCodecError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenCombatRecoveryV1 {
    pub schema_version: u16,
    pub revision: u64,
    pub minimum_remaining: f64,
    pub streak_remaining: f64,
    /// 0=no successful cast, 1=War, 2=Life, 3=Creature, 4=Item, 5=Void.
    pub last_success_school: u8,
    pub last_success_age: f64,
}
impl FrozenCombatRecoveryV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.schema_version != 1
            || self.last_success_school > 5
            || !self.minimum_remaining.is_finite()
            || !(0.0..=4.0).contains(&self.minimum_remaining)
            || !self.streak_remaining.is_finite()
            || !(0.0..=2.0).contains(&self.streak_remaining)
            || !self.last_success_age.is_finite()
            || !(0.0..=5.0).contains(&self.last_success_age)
            || self.last_success_school == 0 && self.last_success_age != 0.0
        {
            return Err(SaveCodecError::Invalid("invalid combat recovery"));
        }
        Ok(())
    }
}
