//! Bounded ACE landblock residency, explicit simulation ticks at 30Hz.
//! A draining region remains owned until the host acknowledges full owner/save
//! teardown. Queue pressure retains the original inactivity deadline.
use std::collections::{BTreeMap, VecDeque};
const DORMANT_AFTER_TICKS: u64 = 30 * 30;
const UNLOAD_AFTER_TICKS: u64 = 5 * 60 * 30;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionPhase {
    Preparing,
    Active,
    Dormant,
    Draining,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyError {
    Capacity,
    Stale,
    Invalid,
    Busy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionLifecycleEvent {
    Prepare { landblock: u16, epoch: u64 },
    Dormant { landblock: u16, epoch: u64 },
    Active { landblock: u16, epoch: u64 },
    Unload { landblock: u16, epoch: u64 },
    Unloaded { landblock: u16, epoch: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionResidencyState {
    pub epoch: u64,
    pub phase: RegionPhase,
    pub permanent: bool,
    pub dungeon: bool,
    pub last_active_tick: u64,
    pub keep_alive_objects: u32,
    next_heartbeat: u64,
}
pub struct RegionResidency {
    regions: BTreeMap<u16, RegionResidencyState>,
    epochs: BTreeMap<u16, u64>,
    events: VecDeque<RegionLifecycleEvent>,
    capacity: usize,
    quiescing: bool,
}
impl RegionResidency {
    pub fn new(capacity: usize) -> Result<Self, ResidencyError> {
        if !(1..=4096).contains(&capacity) {
            return Err(ResidencyError::Capacity);
        }
        Ok(Self {
            regions: BTreeMap::new(),
            epochs: BTreeMap::new(),
            events: VecDeque::with_capacity(capacity),
            capacity,
            quiescing: false,
        })
    }
    pub fn has_state(&self) -> bool {
        !self.regions.is_empty() || !self.events.is_empty()
    }
    /// Trusted shutdown intent. Every admitted region still traverses ordinary
    /// generator retirement, save acknowledgment and geometry eviction.
    pub fn begin_drain(&mut self) {
        self.quiescing = true;
    }
    pub fn is_quiescing(&self) -> bool {
        self.quiescing
    }
    pub fn state(&self, landblock: u16) -> Option<RegionResidencyState> {
        self.regions.get(&landblock).copied()
    }
    pub fn states(&self) -> impl Iterator<Item = (u16, RegionResidencyState)> + '_ {
        self.regions.iter().map(|(&id, &state)| (id, state))
    }
    pub fn request(
        &mut self,
        landblock: u16,
        permanent: bool,
        now: u64,
    ) -> Result<u64, ResidencyError> {
        if self.quiescing {
            return Err(ResidencyError::Busy);
        }
        if let Some(region) = self.regions.get_mut(&landblock) {
            if region.phase == RegionPhase::Draining {
                return Err(ResidencyError::Busy);
            }
            if permanent && !region.permanent && region.phase == RegionPhase::Dormant {
                if self.events.len() == self.capacity {
                    return Err(ResidencyError::Capacity);
                }
                region.phase = RegionPhase::Active;
                self.events.push_back(RegionLifecycleEvent::Active {
                    landblock,
                    epoch: region.epoch,
                });
            }
            region.permanent |= permanent;
            return Ok(region.epoch);
        }
        if self.regions.len() == self.capacity || self.events.len() == self.capacity {
            return Err(ResidencyError::Capacity);
        }
        let epoch = self
            .epochs
            .get(&landblock)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ResidencyError::Capacity)?;
        let next_heartbeat = now.checked_add(150).ok_or(ResidencyError::Invalid)?;
        self.regions.insert(
            landblock,
            RegionResidencyState {
                epoch,
                phase: RegionPhase::Preparing,
                permanent,
                dungeon: false,
                last_active_tick: now,
                keep_alive_objects: 0,
                next_heartbeat,
            },
        );
        self.epochs.insert(landblock, epoch);
        self.events
            .push_back(RegionLifecycleEvent::Prepare { landblock, epoch });
        Ok(epoch)
    }
    pub fn admit(
        &mut self,
        landblock: u16,
        epoch: u64,
        dungeon: bool,
        now: u64,
    ) -> Result<(), ResidencyError> {
        let r = self
            .regions
            .get_mut(&landblock)
            .ok_or(ResidencyError::Stale)?;
        if r.epoch != epoch || r.phase != RegionPhase::Preparing || now < r.last_active_tick {
            return Err(ResidencyError::Stale);
        }
        let heartbeat = now.checked_add(150).ok_or(ResidencyError::Invalid)?;
        r.phase = RegionPhase::Active;
        r.dungeon = dungeon;
        r.last_active_tick = now;
        r.next_heartbeat = heartbeat;
        Ok(())
    }
    /// A failed preparation stays Preparing and can be retried with the same epoch.
    pub fn retry_preparation(&mut self, landblock: u16, epoch: u64) -> Result<(), ResidencyError> {
        let r = self.regions.get(&landblock).ok_or(ResidencyError::Stale)?;
        if r.epoch != epoch || r.phase != RegionPhase::Preparing {
            return Err(ResidencyError::Stale);
        }
        let event = RegionLifecycleEvent::Prepare { landblock, epoch };
        if !self.events.contains(&event) {
            if self.events.len() == self.capacity {
                return Err(ResidencyError::Capacity);
            }
            self.events.push_back(event);
        }
        Ok(())
    }
    pub fn activity(&mut self, landblock: u16, now: u64) -> Result<(), ResidencyError> {
        let r = self.regions.get(&landblock).ok_or(ResidencyError::Stale)?;
        let mut targets = [landblock; 9];
        let mut len = 1;
        if !r.dungeon {
            let x = i32::from(landblock >> 8);
            let y = i32::from(landblock & 255);
            for (dx, dy) in [
                (0, 1),
                (0, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (1, 1),
                (-1, -1),
                (1, -1),
            ] {
                let (x, y) = (x + dx, y + dy);
                if (0..=255).contains(&x) && (0..=255).contains(&y) {
                    targets[len] = ((x as u16) << 8) | y as u16;
                    len += 1;
                }
            }
        }
        let targets = &targets[..len];
        let changes = targets
            .iter()
            .filter_map(|id| self.regions.get(id))
            .filter(|r| r.phase == RegionPhase::Dormant)
            .count();
        if self.events.len() + changes > self.capacity {
            return Err(ResidencyError::Capacity);
        }
        if targets
            .iter()
            .filter_map(|id| self.regions.get(id))
            .any(|r| now < r.last_active_tick || r.phase == RegionPhase::Draining)
        {
            return Err(ResidencyError::Busy);
        }
        for &id in targets {
            if let Some(r) = self.regions.get_mut(&id) {
                r.last_active_tick = now;
                if r.phase == RegionPhase::Dormant {
                    r.phase = RegionPhase::Active;
                    self.events.push_back(RegionLifecycleEvent::Active {
                        landblock: id,
                        epoch: r.epoch,
                    });
                }
            }
        }
        Ok(())
    }
    /// Set the exact accepted WCID80007 count; removal never resets inactivity.
    pub fn keep_alive(
        &mut self,
        landblock: u16,
        epoch: u64,
        count: u32,
    ) -> Result<(), ResidencyError> {
        let r = self
            .regions
            .get_mut(&landblock)
            .ok_or(ResidencyError::Stale)?;
        if r.epoch != epoch || r.phase == RegionPhase::Draining || count > 4096 {
            return Err(ResidencyError::Stale);
        }
        r.keep_alive_objects = count;
        Ok(())
    }
    pub fn advance(&mut self, now: u64) -> Result<(), ResidencyError> {
        if self.regions.values().any(|r| now < r.last_active_tick) {
            return Err(ResidencyError::Invalid);
        }
        let next_heartbeat = now.checked_add(150).ok_or(ResidencyError::Invalid)?;
        for (&id, r) in &mut self.regions {
            if matches!(r.phase, RegionPhase::Preparing | RegionPhase::Draining)
                || (!self.quiescing && now < r.next_heartbeat)
            {
                continue;
            }
            let inactive = now - r.last_active_tick;
            let event = if self.quiescing {
                Some(RegionLifecycleEvent::Unload {
                    landblock: id,
                    epoch: r.epoch,
                })
            } else if !r.permanent && r.keep_alive_objects == 0 {
                if inactive >= UNLOAD_AFTER_TICKS {
                    Some(RegionLifecycleEvent::Unload {
                        landblock: id,
                        epoch: r.epoch,
                    })
                } else if inactive >= DORMANT_AFTER_TICKS && r.phase != RegionPhase::Dormant {
                    Some(RegionLifecycleEvent::Dormant {
                        landblock: id,
                        epoch: r.epoch,
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(event) = event {
                if self.events.len() == self.capacity {
                    continue;
                }
                r.phase = if matches!(event, RegionLifecycleEvent::Unload { .. }) {
                    RegionPhase::Draining
                } else {
                    RegionPhase::Dormant
                };
                self.events.push_back(event);
            }
            r.next_heartbeat = next_heartbeat;
        }
        Ok(())
    }
    pub fn peek_event(&self) -> Option<RegionLifecycleEvent> {
        self.events.front().copied()
    }
    pub fn take_event(&mut self) -> Option<RegionLifecycleEvent> {
        self.events.pop_front()
    }
    pub fn can_confirm_unloaded(&self) -> bool {
        self.events.len() < self.capacity
    }
    pub fn confirm_unloaded(&mut self, landblock: u16, epoch: u64) -> Result<(), ResidencyError> {
        let r = self.regions.get(&landblock).ok_or(ResidencyError::Stale)?;
        if r.epoch != epoch || r.phase != RegionPhase::Draining {
            return Err(ResidencyError::Stale);
        }
        if !self.can_confirm_unloaded() {
            return Err(ResidencyError::Capacity);
        }
        self.regions.remove(&landblock);
        self.events
            .push_back(RegionLifecycleEvent::Unloaded { landblock, epoch });
        Ok(())
    }
}
