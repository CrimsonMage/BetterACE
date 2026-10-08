//! GDLE MonsterAIManager::UpdateMeleeModeAttack/UpdateReturningToSpawn and
//! MonsterAI.h defaults at 353cbab52ef7da2b7063bc3e3f008461d8531693.
//! AGPL-3.0-only, GDLE contributors. Inputs are authoritative accepted state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonsterLeash {
    pub chase_range: f32,
    pub home_range: f32,
    pub arrival_range: f32,
    pub return_timeout_ticks: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnHome {
    Travel,
    Arrived,
    Teleport,
}
impl Default for MonsterLeash {
    fn default() -> Self {
        Self {
            chase_range: 100.0,
            home_range: 150.0,
            arrival_range: 5.0,
            return_timeout_ticks: 900,
        }
    }
}
impl MonsterLeash {
    pub fn valid(self) -> bool {
        [self.chase_range, self.home_range, self.arrival_range]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            && self.arrival_range <= self.home_range
            && self.return_timeout_ticks > 0
            && self.return_timeout_ticks <= u64::from(u32::MAX)
    }
    pub fn exceeded_home(self, distance_squared: f32) -> bool {
        !distance_squared.is_finite() || distance_squared >= self.home_range * self.home_range
    }
    pub fn returning(self, distance_squared: f32, started: u64, now: u64) -> ReturnHome {
        if distance_squared.is_finite()
            && distance_squared < self.arrival_range * self.arrival_range
        {
            return ReturnHome::Arrived;
        }
        if now.saturating_sub(started) >= self.return_timeout_ticks {
            return ReturnHome::Teleport;
        }
        ReturnHome::Travel
    }
}
