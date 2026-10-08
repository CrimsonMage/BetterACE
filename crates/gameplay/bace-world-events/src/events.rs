//! Explicit-time EventManager semantics from pinned ACE, AGPL-3.0-only.
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventState {
    Undefined,
    Enabled,
    Disabled,
    Off,
    On,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDefinition {
    pub name: String,
    pub start: i32,
    pub end: i32,
    pub state: EventState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    Invalid,
    Duplicate,
    Capacity,
    Overflow,
}
#[derive(Debug)]
pub struct Events {
    values: BTreeMap<String, EventDefinition>,
    pk_world: bool,
    revision: u64,
}
fn key(name: &str) -> String {
    name.split('@')
        .next()
        .unwrap_or("")
        .chars()
        .flat_map(char::to_uppercase)
        .collect()
}
impl Events {
    pub fn prepare(definitions: Vec<EventDefinition>, pk_world: bool) -> Result<Self, EventError> {
        if definitions.len() > 65536 {
            return Err(EventError::Capacity);
        }
        let mut values = BTreeMap::new();
        for event in definitions {
            if event.name.is_empty() || event.name.len() > 256 || event.start < -1 || event.end < -1
            {
                return Err(EventError::Invalid);
            }
            if values.insert(key(&event.name), event).is_some() {
                return Err(EventError::Duplicate);
            }
        }
        Ok(Self {
            values,
            pk_world,
            revision: 0,
        })
    }
    pub fn state(&self, name: &str) -> Option<EventState> {
        if key(name) == "EVENTISPKWORLD" {
            Some(if self.pk_world {
                EventState::On
            } else {
                EventState::Off
            })
        } else {
            self.values.get(&key(name)).map(|e| e.state)
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn set(&mut self, name: &str, start: bool) -> Result<bool, EventError> {
        let key = key(name);
        if key == "EVENTISPKWORLD" {
            return Ok(false);
        }
        let Some(event) = self.values.get_mut(&key) else {
            return Ok(false);
        };
        if event.state == EventState::Disabled {
            return Ok(false);
        }
        let next = match (start, event.state) {
            (true, EventState::Enabled | EventState::Off) => EventState::On,
            (false, EventState::Enabled | EventState::On) => EventState::Off,
            _ => event.state,
        };
        if next != event.state {
            let revision = self.revision.checked_add(1).ok_or(EventError::Overflow)?;
            event.state = next;
            self.revision = revision;
        }
        Ok(true)
    }
    pub fn started(&mut self, name: &str, now: i32) -> Result<bool, EventError> {
        if now < 0 {
            return Err(EventError::Invalid);
        }
        let key = key(name);
        if key == "EVENTISPKWORLD" {
            return Ok(self.pk_world);
        }
        let Some(event) = self.values.get(&key) else {
            return Ok(false);
        };
        let state = event.state;
        let start = now > event.start && event.start > -1;
        let end = now > event.end && event.end > -1;
        if state != EventState::Disabled && (event.start != -1 || event.end != -1) {
            if state == EventState::On && end {
                return Ok(!self.set(name, false)?);
            }
            if matches!(state, EventState::Off | EventState::Enabled) && start && !end {
                return self.set(name, true);
            }
        }
        Ok(state == EventState::On)
    }
}
