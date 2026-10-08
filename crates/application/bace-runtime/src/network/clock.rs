//! Explicit protocol time; reliability deadlines remain monotonic milliseconds.
//! The origin comes from admitted world time, not a client echo or a packet.
#[derive(Clone, Copy, Debug)]
pub struct PortalClock {
    origin: f64,
    monotonic_start_ms: u64,
}
impl PortalClock {
    /// ACE.Common.DerethDateTime.MaxValue expression (its comment is four seconds
    /// too high); larger values can crash stock clients.
    pub const MAX_SECONDS: f64 = 1_073_741_824.0;
    pub fn new(origin: f64, monotonic_start_ms: u64) -> Result<Self, &'static str> {
        if !origin.is_finite() || !(0.0..=Self::MAX_SECONDS).contains(&origin) {
            return Err("portal time outside stock-client range");
        }
        Ok(Self {
            origin,
            monotonic_start_ms,
        })
    }
    pub fn at(&self, monotonic_ms: u64) -> Result<f64, &'static str> {
        let elapsed = monotonic_ms
            .checked_sub(self.monotonic_start_ms)
            .ok_or("monotonic clock moved backwards")?;
        let value = self.origin + elapsed as f64 / 1000.0;
        if value > Self::MAX_SECONDS {
            return Err("portal clock exhausted stock-client range");
        }
        Ok(value)
    }
    /// ACE's unchecked ushort projection wraps, unlike Rust's saturating cast.
    pub fn header_time(value: f64) -> u16 {
        value.trunc().rem_euclid(65536.0) as u16
    }
}
