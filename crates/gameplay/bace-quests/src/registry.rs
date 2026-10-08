//! Pinned QuestManager registry mutations. Proposals remain unapplied until owner
//! admission/durability permits adoption; all time inputs are explicit.
mod snapshot;
use crate::{QuestDefinition, QuestEligibility, QuestProgress, next_solve};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestMutation {
    Update,
    Stamp,
    Erase,
    Increment(i32),
    Decrement(i32),
    Completions(i32),
    Bits { mask: i32, on: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestChange {
    pub name: String,
    pub before: Option<QuestProgress>,
    pub after: Option<QuestProgress>,
    pub before_revision: u64,
    pub after_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestRegistryError {
    InvalidName,
    Capacity,
    Overflow,
    Conflict,
    Time,
}
#[derive(Debug)]
pub struct QuestRegistry {
    entries: BTreeMap<String, QuestProgress>,
    revision: u64,
    capacity: usize,
}
pub fn quest_key(name: &str) -> Result<String, QuestRegistryError> {
    let name = name.split('@').next().unwrap_or("");
    if name.is_empty() || name.len() > 256 {
        return Err(QuestRegistryError::InvalidName);
    }
    Ok(name
        .chars()
        .map(|c| {
            let mut it = c.to_uppercase();
            let first = it.next().unwrap_or(c);
            if it.next().is_none() { first } else { c }
        })
        .collect())
}
impl QuestRegistry {
    pub fn new(capacity: usize) -> Result<Self, QuestRegistryError> {
        if capacity == 0 || capacity > 4096 {
            return Err(QuestRegistryError::Capacity);
        }
        Ok(Self {
            entries: BTreeMap::new(),
            revision: 0,
            capacity,
        })
    }
    pub fn get(&self, name: &str) -> Option<QuestProgress> {
        self.entries.get(&quest_key(name).ok()?).copied()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, QuestProgress)> {
        self.entries.iter().map(|(n, p)| (n.as_str(), *p))
    }
    pub fn can_solve(
        &self,
        name: &str,
        definition: Option<&QuestDefinition>,
        now: u32,
        rate: f64,
    ) -> Result<bool, QuestRegistryError> {
        Ok(next_solve(definition, self.get(name).as_ref(), now, rate)
            .map_err(|_| QuestRegistryError::Time)?
            == QuestEligibility::Ready)
    }
    pub fn propose(
        &self,
        name: &str,
        mutation: QuestMutation,
        definition: Option<&QuestDefinition>,
        now: u32,
    ) -> Result<QuestChange, QuestRegistryError> {
        let name = quest_key(name)?;
        let before = self.entries.get(&name).copied();
        let mut after = before;
        let maximum = definition.map_or(-1, |d| d.maximum_solves);
        let stamp = |count: i32| QuestProgress {
            last_completed_seconds: now,
            completions: count,
        };
        match mutation {
            QuestMutation::Erase => after = None,
            QuestMutation::Update | QuestMutation::Stamp | QuestMutation::Increment(_) => {
                let amount = match mutation {
                    QuestMutation::Increment(n) => n,
                    _ => 1,
                };
                if amount > 0 {
                    let first = if before.is_none() { 1 } else { 0 };
                    let current = before.map_or(0, |p| p.completions);
                    let n = i64::from(current) + i64::from(amount);
                    let count = if maximum >= 0 && before.is_some() {
                        n.min(i64::from(maximum)).max(i64::from(current))
                    } else if maximum >= 0 && first == 1 {
                        i64::from(amount).min(i64::from(maximum.max(1)))
                    } else {
                        n
                    };
                    let reached = before.is_some_and(|p| maximum >= 0 && p.completions >= maximum);
                    if !reached {
                        after = Some(stamp(
                            i32::try_from(count).map_err(|_| QuestRegistryError::Overflow)?,
                        ));
                    }
                }
            }
            QuestMutation::Decrement(n) => {
                if let Some(p) = before {
                    after = Some(stamp(
                        p.completions
                            .checked_sub(n)
                            .ok_or(QuestRegistryError::Overflow)?,
                    ));
                }
            }
            QuestMutation::Completions(n) | QuestMutation::Bits { mask: n, .. } => {
                let n = match mutation {
                    QuestMutation::Bits { mask, on } => {
                        let old = before.map_or(0, |p| p.completions);
                        if on { old | mask } else { old & !mask }
                    }
                    _ => n,
                };
                let completion_max = definition.map_or(0, |d| d.maximum_solves);
                let count = if completion_max > -1 {
                    n.min(completion_max)
                } else {
                    n.checked_abs().ok_or(QuestRegistryError::Overflow)?
                };
                after = Some(stamp(count));
            }
        }
        if before.is_none() && after.is_some() && self.entries.len() >= self.capacity {
            return Err(QuestRegistryError::Capacity);
        }
        let after_revision = if before == after {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(QuestRegistryError::Overflow)?
        };
        Ok(QuestChange {
            name,
            before,
            after,
            before_revision: self.revision,
            after_revision,
        })
    }
    pub fn adopt(&mut self, change: QuestChange) -> Result<(), QuestRegistryError> {
        if self.revision != change.before_revision
            || self.entries.get(&change.name).copied() != change.before
            || quest_key(&change.name)? != change.name
        {
            return Err(QuestRegistryError::Conflict);
        }
        let revision = if change.before == change.after {
            self.revision
        } else {
            self.revision
                .checked_add(1)
                .ok_or(QuestRegistryError::Overflow)?
        };
        if revision != change.after_revision {
            return Err(QuestRegistryError::Conflict);
        }
        if change.before.is_none() && change.after.is_some() && self.entries.len() >= self.capacity
        {
            return Err(QuestRegistryError::Capacity);
        }
        if let Some(after) = change.after {
            self.entries.insert(change.name, after);
        } else {
            self.entries.remove(&change.name);
        }
        self.revision = revision;
        Ok(())
    }
}
