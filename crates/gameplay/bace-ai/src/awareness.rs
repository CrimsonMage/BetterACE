//! ACE Monster_Awareness.GetAttackTargets, official 47edade3, AGPL-3.0-only.
//! Spatial visibility/distance and faction identity are authoritative inputs.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetCandidate {
    pub actor: u32,
    pub distance_squared: f32,
    pub attackable: bool,
    pub has_targeting_tactic: bool,
    pub teleporting: bool,
    pub same_faction: bool,
    pub retaliate: bool,
    pub player_or_combat_pet: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Awareness {
    pub current_target: Option<u32>,
    pub visual_range_squared: f32,
    pub chase_range_squared: f32,
    pub target_locked: bool,
    pub monsters_only: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwarenessError {
    InvalidDistance,
    Capacity,
    OutputCapacity,
}
impl Awareness {
    pub fn eligible(&self, target: &TargetCandidate) -> Result<bool, AwarenessError> {
        if [
            self.visual_range_squared,
            self.chase_range_squared,
            target.distance_squared,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(AwarenessError::InvalidDistance);
        }
        let current = self.current_target == Some(target.actor);
        let range = if current {
            self.chase_range_squared
        } else {
            self.visual_range_squared
        };
        Ok((target.attackable || target.has_targeting_tactic)
            && !target.teleporting
            && target.distance_squared <= range
            && (!target.same_faction || target.retaliate)
            && (!self.target_locked || current)
            && (!self.monsters_only || !target.player_or_combat_pet))
    }
    /// Validate the whole query before output mutation; no per-tick allocation.
    pub fn filter(
        &self,
        candidates: &[TargetCandidate],
        output: &mut Vec<u32>,
    ) -> Result<(), AwarenessError> {
        if candidates.len() > 4096 {
            return Err(AwarenessError::Capacity);
        }
        let mut count = 0;
        for candidate in candidates {
            if self.eligible(candidate)? {
                count += 1;
            }
        }
        if output.capacity() - output.len() < count {
            return Err(AwarenessError::OutputCapacity);
        }
        for candidate in candidates {
            if self.eligible(candidate)? {
                output.push(candidate.actor);
            }
        }
        Ok(())
    }
}
