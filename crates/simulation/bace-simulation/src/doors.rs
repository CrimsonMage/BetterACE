//! GDLE-led door authority joined to the same world's synthetic dynamic collider.
//! Prepared animations are immutable inputs, not durations/flags from clients.
mod residency;
use bace_gameplay_api::{DoorChange, DoorRejection};
use bace_interactions::{DoorAction, DoorAuthority, DoorError, DoorMotion, DoorPhysics};
use bace_types::EntityId;
use bace_world::{DoorCollider, World};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedDoorHook {
    pub offset_ticks: u64,
    pub ethereal: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedDoorAnimation {
    pub duration_ticks: u64,
    pub hooks: Vec<PreparedDoorHook>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedDoor {
    pub collider: DoorCollider,
    pub initially_open: bool,
    pub initially_locked: bool,
    pub reset_interval_ticks: Option<u64>,
    pub use_radius: f32,
    pub open: PreparedDoorAnimation,
    pub close: PreparedDoorAnimation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorEvent {
    pub door: EntityId,
    pub motion: Option<DoorMotion>,
    pub physics: Option<DoorPhysics>,
    pub locked: Option<bool>,
}
struct Animation {
    generation: u64,
    start: u64,
    next: usize,
    open: bool,
}
struct Entry {
    prepared: PreparedDoor,
    authority: DoorAuthority,
    animation: Option<Animation>,
}
pub(crate) struct Doors {
    entries: BTreeMap<EntityId, Entry>,
    events: VecDeque<DoorEvent>,
    capacity: usize,
}
impl Doors {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            events: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub(crate) fn can_accept(&self) -> bool {
        self.events.len() < self.capacity
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.entries.is_empty() || !self.events.is_empty()
    }
    pub(crate) fn pending_events(&self) -> usize {
        self.events.len()
    }
    pub(crate) fn take_event(&mut self) -> Option<DoorEvent> {
        self.events.pop_front()
    }
    pub(crate) fn physics(&self, id: EntityId) -> Option<DoorPhysics> {
        self.entries.get(&id).map(|e| e.authority.physics())
    }
    pub(crate) fn register(
        &mut self,
        id: EntityId,
        prepared: PreparedDoor,
        world: &mut World,
    ) -> Result<(), DoorRejection> {
        if self.entries.len() >= self.capacity {
            return Err(DoorRejection::Capacity);
        }
        if self.entries.contains_key(&id) {
            return Err(DoorRejection::InvalidState);
        }
        if !prepared.use_radius.is_finite()
            || prepared.use_radius <= 0.0
            || prepared.use_radius > 192.0
        {
            return Err(DoorRejection::InvalidState);
        }
        for (animation, open) in [(&prepared.open, true), (&prepared.close, false)] {
            if animation.duration_ticks == 0
                || animation
                    .hooks
                    .last()
                    .is_none_or(|hook| hook.ethereal != open)
                || animation.hooks.len() > 64
                || animation
                    .hooks
                    .iter()
                    .any(|h| h.offset_ticks > animation.duration_ticks)
                || animation
                    .hooks
                    .windows(2)
                    .any(|w| w[0].offset_ticks > w[1].offset_ticks)
            {
                return Err(DoorRejection::InvalidState);
            }
        }
        if prepared.collider.solid == prepared.initially_open {
            return Err(DoorRejection::InvalidState);
        }
        world
            .register_door(id, prepared.collider)
            .map_err(|_| DoorRejection::MissingGeometry)?;
        let authority = DoorAuthority::new(
            prepared.initially_open,
            prepared.initially_locked,
            prepared.reset_interval_ticks,
        );
        self.entries.insert(
            id,
            Entry {
                prepared,
                authority,
                animation: None,
            },
        );
        Ok(())
    }
    pub(crate) fn apply(
        &mut self,
        actor: EntityId,
        door: EntityId,
        world: &World,
        tick: u64,
    ) -> Result<DoorChange, DoorRejection> {
        if !self.can_accept() {
            return Err(DoorRejection::Capacity);
        }
        if world.combatant(actor).is_some_and(|c| c.health() == 0) {
            return Err(DoorRejection::InvalidState);
        }
        let entry = self
            .entries
            .get_mut(&door)
            .ok_or(DoorRejection::MissingDoor)?;
        let (range, clear) = world
            .door_use_geometry(actor, door, entry.prepared.use_radius)
            .map_err(|_| DoorRejection::MissingGeometry)?;
        let action = entry
            .authority
            .activate(tick, range, clear)
            .map_err(rejection)?;
        if let DoorAction::Motion(motion) = action {
            Self::started(entry, motion, tick)?;
            self.events.push_back(DoorEvent {
                door,
                motion: Some(motion),
                physics: None,
                locked: None,
            });
        }
        Ok(change(door, action))
    }
    pub(crate) fn scripted_state(
        &mut self,
        door: EntityId,
        open: bool,
        tick: u64,
    ) -> Result<DoorChange, DoorRejection> {
        if !self.can_accept() {
            return Err(DoorRejection::Capacity);
        }
        let entry = self
            .entries
            .get_mut(&door)
            .ok_or(DoorRejection::MissingDoor)?;
        let action = entry
            .authority
            .scripted_state(open, tick)
            .map_err(rejection)?;
        if let DoorAction::Motion(motion) = action {
            Self::started(entry, motion, tick)?;
            self.events.push_back(DoorEvent {
                door,
                motion: Some(motion),
                physics: None,
                locked: None,
            });
        }
        Ok(change(door, action))
    }
    fn started(entry: &mut Entry, motion: DoorMotion, tick: u64) -> Result<(), DoorRejection> {
        entry
            .authority
            .motion_started(motion.generation)
            .map_err(rejection)?;
        entry.animation = Some(Animation {
            generation: motion.generation,
            start: tick,
            next: 0,
            open: motion.open,
        });
        Ok(())
    }
    pub(crate) fn npc_contacts(&mut self, world: &World, tick: u64) {
        for (actor, _, _) in world.states() {
            if !self.can_accept() {
                break;
            }
            if world
                .combatant(actor)
                .is_none_or(|c| c.profile().player || c.health() == 0)
            {
                continue;
            }
            let Some(id) = world
                .body(actor)
                .ok()
                .and_then(|b| b.dynamic_contact())
                .map(EntityId)
            else {
                continue;
            };
            let Some(entry) = self.entries.get_mut(&id) else {
                continue;
            };
            if let Ok(DoorAction::Motion(motion)) =
                entry.authority.monster_contact(tick, false, true)
                && Self::started(entry, motion, tick).is_ok()
            {
                self.events.push_back(DoorEvent {
                    door: id,
                    motion: Some(motion),
                    physics: None,
                    locked: None,
                });
            }
        }
    }
    pub(crate) fn step(&mut self, world: &mut World, tick: u64) {
        for (&door, entry) in &mut self.entries {
            if self.events.len() == self.capacity {
                break;
            }
            let Ok(overlapping) = world.door_occupied(door) else {
                continue;
            };
            let old_physics = entry.authority.physics();
            let old_locked = entry.authority.locked();
            let mut motion = None;
            if let Some(animation) = entry.animation.as_mut() {
                let prepared = if animation.open {
                    &entry.prepared.open
                } else {
                    &entry.prepared.close
                };
                while let Some(hook) = prepared.hooks.get(animation.next) {
                    if tick.saturating_sub(animation.start) < hook.offset_ticks {
                        break;
                    }
                    if entry
                        .authority
                        .ethereal_hook(animation.generation, hook.ethereal, true, overlapping)
                        .is_err()
                    {
                        break;
                    }
                    animation.next += 1;
                }
                if tick.saturating_sub(animation.start) >= prepared.duration_ticks
                    && animation.next == prepared.hooks.len()
                {
                    let _ = entry.authority.motion_finished(animation.generation);
                    entry.animation = None;
                }
            }
            if entry.authority.physics().retry_solidity {
                let _ = entry.authority.retry_solidity(true, overlapping);
            }
            if let Ok(reset) = entry.authority.poll_reset(tick)
                && let DoorAction::Motion(command) = reset.action
                && Self::started(entry, command, tick).is_ok()
            {
                motion = Some(command);
            }
            let physics = entry.authority.physics();
            if physics != old_physics && world.set_door_solid(door, !physics.ethereal).is_err() {
                continue;
            }
            let physics = (physics != old_physics).then_some(physics);
            let locked =
                (entry.authority.locked() != old_locked).then_some(entry.authority.locked());
            if motion.is_some() || physics.is_some() || locked.is_some() {
                self.events.push_back(DoorEvent {
                    door,
                    motion,
                    physics,
                    locked,
                });
            }
        }
    }
}
fn change(door: EntityId, action: DoorAction) -> DoorChange {
    match action {
        DoorAction::Motion(m) => DoorChange::Motion {
            door,
            open: m.open,
            generation: m.generation,
            server_control: m.server_control,
            animation_sequence: m.animation_sequence,
        },
        DoorAction::Busy => DoorChange::Busy,
        DoorAction::Locked => DoorChange::Locked,
        DoorAction::Unchanged => DoorChange::Unchanged,
    }
}
fn rejection(error: DoorError) -> DoorRejection {
    match error {
        DoorError::OutOfRange => DoorRejection::OutOfRange,
        DoorError::Obstructed => DoorRejection::Obstructed,
        DoorError::MissingGeometry => DoorRejection::MissingGeometry,
        DoorError::StaleAnimation | DoorError::Overflow => DoorRejection::InvalidState,
    }
}
