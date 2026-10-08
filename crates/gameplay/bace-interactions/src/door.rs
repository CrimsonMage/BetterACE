//! GDLE-led door authority, pin 353cbab52ef7da2b7063bc3e3f008461d8531693.
//! Door.cpp owns nonautonomous motion and lock transitions; PhatSDK/PhysicsObj.cpp
//! set_ethereal owns deferred solidity. AGPL-3.0-only, GDLE contributors.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorMotion {
    pub generation: u64,
    pub open: bool,
    pub server_control: u16,
    pub animation_sequence: u16,
    pub autonomous: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorAction {
    Unchanged,
    Busy,
    Locked,
    Motion(DoorMotion),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorError {
    OutOfRange,
    Obstructed,
    MissingGeometry,
    StaleAnimation,
    Overflow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorReset {
    pub lock_changed: bool,
    pub action: DoorAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorPhysics {
    pub ethereal: bool,
    pub retry_solidity: bool,
}

/// Commanded motion, interpreted motion and physical solidity are distinct.
/// Only trusted simulation/physics adapters may provide range, overlap and hooks;
/// no client position/contact or client-selected open flag belongs in this API.
#[derive(Clone, Debug)]
pub struct DoorAuthority {
    requested_open: bool,
    interpreted_open: bool,
    initial_open: bool,
    locked: bool,
    initial_locked: bool,
    generation: u64,
    server_control: u16,
    animation_sequence: u16,
    animating: bool,
    physics: DoorPhysics,
    reset_interval: Option<u64>,
    reset_at: Option<u64>,
}
impl DoorAuthority {
    pub fn new(initial_open: bool, initial_locked: bool, reset_interval: Option<u64>) -> Self {
        Self {
            requested_open: initial_open,
            interpreted_open: initial_open,
            initial_open,
            locked: initial_locked,
            initial_locked,
            generation: 0,
            server_control: 0,
            animation_sequence: 0,
            animating: false,
            physics: DoorPhysics {
                ethereal: initial_open,
                retry_solidity: false,
            },
            reset_interval,
            reset_at: None,
        }
    }
    pub fn requested_open(&self) -> bool {
        self.requested_open
    }
    /// Trusted authored OpenMe/CloseMe uses an explicit state, independently of
    /// client use range and locks. Motion hooks still own collision solidity.
    pub fn scripted_state(&mut self, open: bool, now: u64) -> Result<DoorAction, DoorError> {
        self.request(open, now)
    }
    /// GDLE CBaseDoor::IsClosed tests interpreted motion, not physical solidity.
    pub fn is_closed(&self) -> bool {
        !self.interpreted_open
    }
    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn physics(&self) -> DoorPhysics {
        self.physics
    }
    /// An authorized lock/key subsystem changes this state; activation itself
    /// never bypasses the lock based on which side a client claims to occupy.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }
    pub fn activate(
        &mut self,
        now: u64,
        in_range: bool,
        geometry_clear: bool,
    ) -> Result<DoorAction, DoorError> {
        if !in_range {
            return Err(DoorError::OutOfRange);
        }
        if !geometry_clear {
            return Err(DoorError::Obstructed);
        }
        if !self.requested_open && self.locked {
            return Ok(DoorAction::Locked);
        }
        self.request(!self.requested_open, now)
    }
    /// GDLE Monster::DoCollision permits non-player creatures to open an
    /// unlocked closed door. Caller must supply an actual authoritative contact.
    pub fn monster_contact(
        &mut self,
        now: u64,
        is_player: bool,
        authoritative_contact: bool,
    ) -> Result<DoorAction, DoorError> {
        if is_player || !authoritative_contact || self.locked || !self.is_closed() {
            return Ok(DoorAction::Unchanged);
        }
        self.request(true, now)
    }
    fn request(&mut self, open: bool, now: u64) -> Result<DoorAction, DoorError> {
        if self.animating {
            return Ok(DoorAction::Busy);
        }
        if self.requested_open == open {
            return Ok(DoorAction::Unchanged);
        }
        let generation = self.generation.checked_add(1).ok_or(DoorError::Overflow)?;
        let reset_at = if open || self.initial_open {
            self.reset_interval
                .map(|delay| now.checked_add(delay).ok_or(DoorError::Overflow))
                .transpose()?
        } else {
            self.reset_at
        };
        self.generation = generation;
        self.requested_open = open;
        self.animating = true;
        self.server_control = self.server_control.wrapping_add(1);
        self.animation_sequence = self.animation_sequence.wrapping_add(1);
        self.reset_at = reset_at;
        // A re-open invalidates pending work from the preceding close. The
        // authoritative opening hook establishes ethereality, not this request.
        if open {
            self.physics.retry_solidity = false;
        }
        Ok(DoorAction::Motion(DoorMotion {
            generation,
            open,
            server_control: self.server_control,
            animation_sequence: self.animation_sequence,
            autonomous: false,
        }))
    }
    pub fn motion_started(&mut self, generation: u64) -> Result<(), DoorError> {
        self.check_generation(generation)?;
        self.interpreted_open = self.requested_open;
        Ok(())
    }
    pub fn motion_finished(&mut self, generation: u64) -> Result<(), DoorError> {
        self.check_generation(generation)?;
        self.animating = false;
        Ok(())
    }
    fn check_generation(&self, generation: u64) -> Result<(), DoorError> {
        if generation != self.generation || generation == 0 {
            Err(DoorError::StaleAnimation)
        } else {
            Ok(())
        }
    }
    /// Execute an asset-provided EtherealHook with authoritative occupancy. A
    /// closed-looking door remains ethereal while occupied, exactly as GDLE's
    /// CHECK_ETHEREAL retry path. Missing geometry fails closed without mutation.
    pub fn ethereal_hook(
        &mut self,
        generation: u64,
        ethereal: bool,
        geometry_ready: bool,
        overlapping: bool,
    ) -> Result<DoorPhysics, DoorError> {
        self.check_generation(generation)?;
        if !geometry_ready {
            return Err(DoorError::MissingGeometry);
        }
        self.physics = if ethereal {
            DoorPhysics {
                ethereal: true,
                retry_solidity: false,
            }
        } else if overlapping {
            DoorPhysics {
                ethereal: true,
                retry_solidity: true,
            }
        } else {
            DoorPhysics {
                ethereal: false,
                retry_solidity: false,
            }
        };
        Ok(self.physics)
    }
    /// Run on the simulation physics cadence, never a wall-clock callback.
    pub fn retry_solidity(
        &mut self,
        geometry_ready: bool,
        overlapping: bool,
    ) -> Result<DoorPhysics, DoorError> {
        if !geometry_ready {
            return Err(DoorError::MissingGeometry);
        }
        if self.physics.retry_solidity && !overlapping {
            self.physics = DoorPhysics {
                ethereal: false,
                retry_solidity: false,
            };
        }
        Ok(self.physics)
    }
    pub fn poll_reset(&mut self, now: u64) -> Result<DoorReset, DoorError> {
        if self.reset_at.is_none_or(|due| now < due) {
            return Ok(DoorReset {
                lock_changed: false,
                action: DoorAction::Unchanged,
            });
        }
        let lock_changed = self.locked != self.initial_locked;
        let action = self.request(self.initial_open, now)?;
        self.locked = self.initial_locked;
        if action != DoorAction::Busy {
            self.reset_at = None;
        }
        Ok(DoorReset {
            lock_changed,
            action,
        })
    }
}
