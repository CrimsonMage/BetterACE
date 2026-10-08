//! Pinned PlayerManager.GagPlayer/UnGagPlayer and Player_Tick.GagsTick.
//! Remaining duration advances only on explicit online heartbeat inputs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GagState {
    pub active: bool,
    pub timestamp: f64,
    pub remaining: f64,
    /// Source transient gagNoticeSent; reset on reconstruction only, or expiry.
    pub noticed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GagError {
    Invalid,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GagNotices {
    pub suspended: bool,
    pub restored: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GagChange {
    pub before: GagState,
    pub after: GagState,
    pub notices: GagNotices,
}
impl GagState {
    pub fn validate(self) -> Result<(), GagError> {
        if !self.timestamp.is_finite()
            || self.timestamp < 0.
            || !self.remaining.is_finite()
            || self.remaining < 0.
        {
            return Err(GagError::Invalid);
        }
        Ok(())
    }
    pub fn change(self, enabled: bool, unix_seconds: f64) -> Result<GagChange, GagError> {
        self.validate()?;
        if !unix_seconds.is_finite() || unix_seconds < 0. {
            return Err(GagError::Invalid);
        }
        Ok(GagChange {
            before: self,
            after: Self {
                active: enabled,
                timestamp: if enabled { unix_seconds } else { 0. },
                remaining: if enabled { 300. } else { 0. },
                noticed: self.noticed,
            },
            notices: Default::default(),
        })
    }
    pub fn heartbeat(self, interval: f64) -> Result<GagChange, GagError> {
        self.validate()?;
        if !interval.is_finite() || interval <= 0. {
            return Err(GagError::Invalid);
        }
        let mut after = self;
        let mut notices = GagNotices::default();
        if self.active {
            notices.suspended = !self.noticed;
            after.noticed = true;
            after.remaining -= interval;
            if after.remaining <= 0. {
                after.active = false;
                after.timestamp = 0.;
                after.remaining = 0.;
                after.noticed = false;
                notices.restored = true;
            }
        }
        Ok(GagChange {
            before: self,
            after,
            notices,
        })
    }
}
impl GagChange {
    pub fn durable_changed(self) -> bool {
        (
            self.before.active,
            self.before.timestamp,
            self.before.remaining,
        ) != (
            self.after.active,
            self.after.timestamp,
            self.after.remaining,
        )
    }
    pub fn apply(self, state: &mut GagState) -> Result<(), GagError> {
        if *state != self.before {
            return Err(GagError::Invalid);
        }
        self.after.validate()?;
        *state = self.after;
        Ok(())
    }
}
