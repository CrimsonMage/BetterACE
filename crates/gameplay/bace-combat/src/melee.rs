//! Bounded authoritative attack-hook scheduling. ACE Monster_Melee.cs schedules
//! each strike at attackFrames[i].time * animLength, and checks life at impact.
//! Geometry/range rechecks are intentional server-authority hardening (#7/#9/#10).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub offset_seconds: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackError {
    InvalidTime,
    InvalidHooks,
    InvalidTarget,
    Capacity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackImpact {
    Waiting,
    Strike { index: usize },
    Cancelled,
    Complete,
}

/// Scheduling only: damage formulas, animation selection and permission checks
/// are supplied by their owners. A caller cannot convert this into pose authority.
pub struct MeleeAttack {
    target: u32,
    start: f64,
    end: f64,
    hooks: Vec<Strike>,
    next: usize,
    cancelled: bool,
}
impl MeleeAttack {
    pub fn new(
        target: u32,
        start: f64,
        duration: f64,
        hooks: &[Strike],
    ) -> Result<Self, AttackError> {
        if target == 0 {
            return Err(AttackError::InvalidTarget);
        }
        if !start.is_finite()
            || start < 0.0
            || !duration.is_finite()
            || duration <= 0.0
            || !(start + duration).is_finite()
        {
            return Err(AttackError::InvalidTime);
        }
        if hooks.is_empty() || hooks.len() > 32 {
            return Err(AttackError::Capacity);
        }
        if hooks.iter().any(|h| {
            !h.offset_seconds.is_finite() || h.offset_seconds < 0.0 || h.offset_seconds > duration
        }) || hooks
            .windows(2)
            .any(|pair| pair[0].offset_seconds > pair[1].offset_seconds)
        {
            return Err(AttackError::InvalidHooks);
        }
        Ok(Self {
            target,
            start,
            end: start + duration,
            hooks: hooks.to_vec(),
            next: 0,
            cancelled: false,
        })
    }
    pub fn target(&self) -> u32 {
        self.target
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
    /// Poll one due strike at a time to keep work/output budgets explicit.
    pub fn poll(
        &mut self,
        now: f64,
        attacker_alive: bool,
        target_alive: bool,
        in_range: bool,
        geometry_clear: bool,
    ) -> Result<AttackImpact, AttackError> {
        if !now.is_finite() || now < self.start {
            return Err(AttackError::InvalidTime);
        }
        if self.cancelled {
            return Ok(AttackImpact::Cancelled);
        }
        if !attacker_alive || !target_alive || !in_range || !geometry_clear {
            self.cancelled = true;
            return Ok(AttackImpact::Cancelled);
        }
        if let Some(hook) = self.hooks.get(self.next)
            && now >= self.start + hook.offset_seconds
        {
            let index = self.next;
            self.next += 1;
            return Ok(AttackImpact::Strike { index });
        }
        Ok(if self.next == self.hooks.len() && now >= self.end {
            AttackImpact::Complete
        } else {
            AttackImpact::Waiting
        })
    }
}
