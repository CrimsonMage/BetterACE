//! ACE Player_Tick.ManaConsumersTick and WorldObject_Magic activation state.
//! Accumulator/warning/action delay are transient ACE fields. Recovery retains
//! them; durable reconstruction intentionally resets them, preserving Biota mana.
use bace_types::EntityId;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentManaItem {
    pub item: EntityId,
    pub name: String,
    pub current: Option<i32>,
    pub maximum: Option<i32>,
    pub rate: Option<f64>,
    pub affecting: Option<bool>,
    /// Every known spell (including SpellDID/proc), with source RemoveItemSpell target.
    pub removals: Vec<(EntityId, u32)>,
    pub accumulator: f32,
    pub warned: bool,
    /// Seconds remaining; no offline advancement on a transferred owner.
    pub removal_remaining: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentManaRecovery {
    /// True only for cold reconstruction; simulation derives the source 0..5s spread.
    pub fresh: bool,
    pub heartbeat: f64,
    pub rating: u32,
    pub heartbeat_remaining: f64,
    pub items: Vec<EquipmentManaItem>,
}
pub(crate) struct EquipmentManaPlayer {
    pub(crate) entry_cleanup: bool,
    pub(crate) heartbeat: f64,
    pub(crate) rating: u32,
    pub(crate) next: f64,
    pub(crate) items: BTreeMap<EntityId, EquipmentManaItem>,
    pub(crate) removals: BTreeMap<EntityId, f64>,
    pub(crate) changes: Vec<(EntityId, ManaValues, Option<bool>)>,
    pub(crate) due: Vec<EntityId>,
}
pub(crate) struct EquipmentMana {
    pub(crate) players: BTreeMap<EntityId, EquipmentManaPlayer>,
    pub(crate) capacity: usize,
    pub(crate) scratch: Vec<EntityId>,
}
impl EquipmentMana {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            players: BTreeMap::new(),
            capacity: capacity.min(4096),
            scratch: Vec::new(),
        }
    }
}
impl EquipmentManaRecovery {
    pub fn validate(&self, actor: EntityId) -> Result<(), &'static str> {
        if actor.0 == 0
            || self.items.len() > 1024
            || !self.heartbeat.is_finite()
            || self.heartbeat < 0.
            || self.heartbeat > 86400.
            || !self.heartbeat_remaining.is_finite()
            || self.heartbeat_remaining > self.heartbeat.max(5.)
        {
            return Err("equipment mana heartbeat");
        }
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.items {
            if item.item.0 == 0
                || item.item == actor
                || !seen.insert(item.item)
                || item.name.len() > 1024
                || item.removals.len() > 256
                || item.current.is_some_and(|n| n < 0)
                || item.maximum.is_some_and(|n| n < 0)
                || item.rate.is_some_and(|rate| !rate.is_finite())
                || !item.accumulator.is_finite()
                || item
                    .removal_remaining
                    .is_some_and(|time| !time.is_finite() || !(0. ..=2.).contains(&time))
                || item
                    .removals
                    .iter()
                    .any(|(target, spell)| ![actor, item.item].contains(target) || *spell == 0)
            {
                return Err("equipment mana item");
            }
        }
        Ok(())
    }
}
/// Arithmetic order matches the pinned C# double multiplication -> float cast
/// -> float addition. No elapsed-wall-clock extrapolation or integer rounding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ManaValues {
    pub(crate) current: Option<i32>,
    pub(crate) affecting: Option<bool>,
    pub(crate) accumulator: f32,
    pub(crate) warned: bool,
    pub(crate) removal_remaining: Option<f64>,
}
impl ManaValues {
    pub(crate) fn apply(self, item: &mut EquipmentManaItem) {
        item.current = self.current;
        item.affecting = self.affecting;
        item.accumulator = self.accumulator;
        item.warned = self.warned;
        item.removal_remaining = self.removal_remaining;
    }
}
pub(crate) fn heartbeat(
    item: &EquipmentManaItem,
    interval: f64,
    rating: u32,
) -> Result<(ManaValues, Option<bool>), &'static str> {
    let mut after = ManaValues {
        current: item.current,
        affecting: item.affecting,
        accumulator: item.accumulator,
        warned: item.warned,
        removal_remaining: item.removal_remaining,
    };
    if item.affecting != Some(true)
        || item.current.is_none()
        || item.maximum.is_none()
        || item.rate.is_none()
    {
        return Ok((after, None));
    }
    let mut burn_rate = -item.rate.unwrap();
    if rating != 0 {
        burn_rate *= f64::from(
            100.0f32
                / (100u32
                    .checked_add(rating)
                    .ok_or("equipment mana rating overflow")? as f32),
        );
    }
    after.accumulator += (burn_rate * interval) as f32;
    if !after.accumulator.is_finite() {
        return Err("equipment mana accumulator overflow");
    }
    if after.accumulator < 1. {
        return Ok((after, None));
    }
    let floor = f64::from(after.accumulator).floor();
    if floor > f64::from(i32::MAX) {
        return Err("equipment mana burn overflow");
    }
    let burn = (floor as i32).min(item.current.unwrap());
    let current = item.current.unwrap() - burn;
    after.current = Some(current);
    after.accumulator -= burn as f32;
    if current == 0 {
        after.affecting = None;
        after.removal_remaining = Some(2.);
        return Ok((after, Some(true)));
    }
    if f64::from(current) / burn_rate > 120. {
        after.warned = false;
        return Ok((after, None));
    }
    let notice = (!after.warned).then_some(false);
    after.warned = true;
    Ok((after, notice))
}

#[cfg(test)]
mod tests;
