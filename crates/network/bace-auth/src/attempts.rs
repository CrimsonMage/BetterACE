use std::{collections::BTreeMap, net::IpAddr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptError {
    InvalidLimits,
    InvalidClock,
    RateLimited,
    Capacity,
}

/// Fixed-window admission before session allocation or password work. Entries
/// expire without refreshing their original window on rejected attempts.
pub struct LoginAttempts {
    addresses: BTreeMap<IpAddr, (u64, u32)>,
    max_addresses: usize,
    max_attempts: u32,
    window_ms: u64,
    last_ms: u64,
}

impl LoginAttempts {
    pub fn new(
        max_addresses: usize,
        max_attempts: u32,
        window_ms: u64,
    ) -> Result<Self, AttemptError> {
        if !(1..=65536).contains(&max_addresses)
            || !(1..=1000).contains(&max_attempts)
            || window_ms == 0
        {
            return Err(AttemptError::InvalidLimits);
        }
        Ok(Self {
            addresses: BTreeMap::new(),
            max_addresses,
            max_attempts,
            window_ms,
            last_ms: 0,
        })
    }
    pub fn admit(&mut self, address: IpAddr, now_ms: u64) -> Result<(), AttemptError> {
        if now_ms < self.last_ms {
            return Err(AttemptError::InvalidClock);
        }
        self.last_ms = now_ms;
        // Prune only when capacity is needed. Work is bounded by max_addresses.
        if !self.addresses.contains_key(&address) && self.addresses.len() == self.max_addresses {
            self.addresses
                .retain(|_, (start, _)| now_ms - *start < self.window_ms);
        }
        if let Some((start, count)) = self.addresses.get_mut(&address) {
            if now_ms - *start >= self.window_ms {
                *start = now_ms;
                *count = 0;
            }
            if *count == self.max_attempts {
                return Err(AttemptError::RateLimited);
            }
            *count += 1;
        } else {
            if self.addresses.len() == self.max_addresses {
                return Err(AttemptError::Capacity);
            }
            self.addresses.insert(address, (now_ms, 1));
        }
        Ok(())
    }
    pub fn tracked_addresses(&self) -> usize {
        self.addresses.len()
    }
}
