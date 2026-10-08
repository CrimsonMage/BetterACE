//! ACE EnchantmentManager.Add, AddEnchantmentResult and registry top-layer rules.
//! Explicit time and bounded state replace upstream clocks/locks/database objects.
use crate::{DispelSpec, EnchantmentSpec, MagicSchool};
mod vitae;
pub use vitae::VitaeMutation;
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentEntry {
    pub spell: u32,
    pub caster: u32,
    pub school: MagicSchool,
    pub spec: EnchantmentSpec,
    pub start_time: f64,
    pub is_set_spell: bool,
    pub is_level8_aura: bool,
    /// Stored ACE fields which are independent of prepared spell behavior.
    pub metadata: EnchantmentMetadata,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnchantmentMetadata {
    pub enchantment_category: u32,
    pub has_spell_set_id: bool,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    pub last_time_degraded: f64,
    /// Preserve the signed stored value even when has_spell_set_id is false.
    pub spell_set_id: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnchantmentStack {
    Initial,
    Surpass,
    Refresh,
    Surpassed,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentApplication {
    pub entry: EnchantmentEntry,
    pub stack: EnchantmentStack,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryError {
    Capacity,
    InvalidEntry,
    InvalidTime,
    LayerExhausted,
    RevisionExhausted,
    MissingEntry,
    OutputCapacity,
}
pub struct PreparedEnchantment {
    before_revision: u64,
    replace: Option<usize>,
    application: EnchantmentApplication,
}

#[derive(Debug)]
pub struct EnchantmentRegistry {
    entries: Vec<EnchantmentEntry>,
    capacity: usize,
    revision: u64,
}
impl EnchantmentRegistry {
    /// Restore exact rows without reapplying stacking or resetting elapsed time.
    /// Failure returns all input rows to the loading owner.
    pub fn restore(
        capacity: usize,
        revision: u64,
        entries: Vec<EnchantmentEntry>,
    ) -> Result<Self, (RegistryError, Vec<EnchantmentEntry>)> {
        if !(1..=4096).contains(&capacity) || entries.len() > capacity {
            return Err((RegistryError::Capacity, entries));
        }
        for (index, entry) in entries.iter().enumerate() {
            if validate_entry(entry).is_err()
                || entries[..index]
                    .iter()
                    .any(|e| (e.spell, e.spec.layer) == (entry.spell, entry.spec.layer))
            {
                return Err((RegistryError::InvalidEntry, entries));
            }
        }
        // Capacity is a logical gameplay bound; empty restored item registries
        // need not allocate thousands of unused rows before their first effect.
        Ok(Self {
            entries,
            capacity,
            revision,
        })
    }
    pub fn into_entries(self) -> Vec<EnchantmentEntry> {
        self.entries
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Pinned login registry construction removes a fully recovered vitae row.
    /// Keep that mutation in the owning domain, never in a network serializer.
    pub fn normalize_vitae(&mut self) -> Result<bool, RegistryError> {
        let recovered = |entry: &EnchantmentEntry| {
            entry.spell == 666
                && (entry.spec.value > 1.0 || (entry.spec.value - 1.0).abs() < 0.0001)
        };
        if !self.entries.iter().any(recovered) {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(RegistryError::RevisionExhausted)?;
        self.entries.retain(|entry| !recovered(entry));
        self.revision = revision;
        Ok(true)
    }
    pub fn new(capacity: usize) -> Result<Self, RegistryError> {
        if !(1..=4096).contains(&capacity) {
            return Err(RegistryError::Capacity);
        }
        Ok(Self {
            // Capacity is a logical bound. Empty world/item registries allocate
            // entries only on first accepted effect, matching restored registries.
            entries: Vec::new(),
            capacity,
            revision: 0,
        })
    }
    pub fn entries(&self) -> &[EnchantmentEntry] {
        &self.entries
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn propose_add(
        &self,
        mut incoming: EnchantmentEntry,
        now: f64,
        equipped: bool,
    ) -> Result<PreparedEnchantment, RegistryError> {
        if !now.is_finite() || now < 0.0 {
            return Err(RegistryError::InvalidTime);
        }
        validate_entry(&incoming)?;
        if incoming.spell == 0
            || incoming.spell > u32::from(u16::MAX)
            || !incoming.spec.duration.is_finite()
            || incoming.spec.duration < 0.0
            || !(now + incoming.spec.duration).is_finite()
            || !incoming.spec.value.is_finite()
        {
            return Err(RegistryError::InvalidEntry);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(RegistryError::RevisionExhausted)?;
        let mut top = 0_u16;
        let mut refresh = None;
        let mut classification = EnchantmentStack::Initial;
        for (index, entry) in self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.spec.category == incoming.spec.category)
        {
            top = top.max(entry.spec.layer);
            let kind = if incoming.spec.power > entry.spec.power {
                EnchantmentStack::Surpass
            } else if incoming.spec.power < entry.spec.power {
                EnchantmentStack::Surpassed
            } else if incoming.spell == entry.spell {
                if incoming.caster == entry.caster {
                    refresh = Some(index);
                }
                EnchantmentStack::Refresh
            } else {
                let new_duration = if equipped {
                    f64::INFINITY
                } else {
                    incoming.spec.duration
                };
                let old_duration = if entry.spec.duration == -1.0 {
                    f64::INFINITY
                } else {
                    entry.spec.duration
                };
                if new_duration > old_duration
                    || (new_duration == old_duration && !entry.is_set_spell)
                    || (new_duration == old_duration && incoming.spell > entry.spell)
                {
                    EnchantmentStack::Surpass
                } else {
                    EnchantmentStack::Surpassed
                }
            };
            if matches!(kind, EnchantmentStack::Surpassed)
                || classification != EnchantmentStack::Surpassed
                    && (matches!(kind, EnchantmentStack::Refresh)
                        || classification != EnchantmentStack::Refresh)
            {
                classification = kind;
            }
        }
        let entry = if let Some(index) = refresh {
            let mut existing = self.entries[index].clone();
            let remaining = existing.spec.duration + existing.start_time;
            if incoming.spec.duration > remaining {
                existing.start_time = 0.0;
                existing.spec.duration = incoming.spec.duration;
            }
            existing.clone()
        } else {
            if self.entries.len() == self.capacity {
                return Err(RegistryError::Capacity);
            }
            incoming.spec.layer = top.checked_add(1).ok_or(RegistryError::LayerExhausted)?;
            incoming.start_time = 0.0;
            if equipped {
                incoming.spec.duration = -1.0;
            }
            incoming
        };
        Ok(PreparedEnchantment {
            before_revision: self.revision,
            replace: refresh,
            application: EnchantmentApplication {
                entry,
                stack: classification,
                revision,
            },
        })
    }
    pub fn adopt(
        &mut self,
        proposal: PreparedEnchantment,
    ) -> Result<EnchantmentApplication, RegistryError> {
        if proposal.before_revision != self.revision {
            return Err(RegistryError::RevisionExhausted);
        }
        if let Some(index) = proposal.replace {
            let slot = self
                .entries
                .get_mut(index)
                .ok_or(RegistryError::MissingEntry)?;
            *slot = proposal.application.entry.clone();
        } else {
            if self.entries.len() == self.capacity {
                return Err(RegistryError::Capacity);
            }
            self.entries.push(proposal.application.entry.clone());
        }
        self.revision = proposal.application.revision;
        Ok(proposal.application)
    }
    pub fn add(
        &mut self,
        incoming: EnchantmentEntry,
        now: f64,
        equipped: bool,
    ) -> Result<EnchantmentApplication, RegistryError> {
        let proposal = self.propose_add(incoming, now, equipped)?;
        self.adopt(proposal)
    }
    pub fn top(&self, category: u16, _now: f64) -> Option<&EnchantmentEntry> {
        // ACE's stable OrderBy(...).First() retains the earliest exact tie;
        // Iterator::max_by keeps the last, so visit ties in reverse order.
        self.entries
            .iter()
            .rev()
            .filter(|e| e.spec.category == category)
            .max_by(|a, b| {
                a.spec
                    .power
                    .cmp(&b.spec.power)
                    .then(a.is_level8_aura.cmp(&b.is_level8_aura))
                    .then_with(|| {
                        let key = |e: &EnchantmentEntry| {
                            if e.is_set_spell {
                                f64::from(e.spell)
                            } else {
                                e.start_time
                            }
                        };
                        key(a).total_cmp(&key(b))
                    })
            })
    }
    /// ACE advances registry-relative time by the active object's heartbeat,
    /// including indefinite entries. Offline objects must not call this method.
    pub fn heartbeat(
        &mut self,
        interval: f64,
        out: &mut Vec<(u32, u16)>,
    ) -> Result<(), RegistryError> {
        let count = self.expired_count(interval)?;
        let expired = |e: &EnchantmentEntry| {
            e.spec.duration >= 0.0 && e.start_time - interval <= -e.spec.duration
        };
        if count > out.capacity() - out.len() {
            return Err(RegistryError::OutputCapacity);
        }
        if self.entries.is_empty() || interval == 0.0 {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(RegistryError::RevisionExhausted)?;
        self.entries.retain_mut(|entry| {
            let remove = expired(entry);
            entry.start_time -= interval;
            if remove {
                out.push((entry.spell, entry.spec.layer));
                false
            } else {
                true
            }
        });
        self.revision = revision;
        Ok(())
    }
    /// Preflight bounded output before any clock or registry mutation.
    pub fn expired_count(&self, interval: f64) -> Result<usize, RegistryError> {
        if !interval.is_finite()
            || interval < 0.0
            || self
                .entries
                .iter()
                .any(|e| !(e.start_time - interval).is_finite())
        {
            return Err(RegistryError::InvalidTime);
        }
        Ok(self
            .entries
            .iter()
            .filter(|e| e.spec.duration >= 0.0 && e.start_time - interval <= -e.spec.duration)
            .count())
    }
    /// Source filtering precedes caller-controlled shuffle/count draws. Equipped
    /// indefinite spells are excluded, and every candidate layer is retained.
    pub fn dispel_candidates(
        &self,
        spec: &DispelSpec,
        out: &mut Vec<(u32, u16)>,
    ) -> Result<(), RegistryError> {
        let eligible = |e: &EnchantmentEntry| {
            e.spec.duration != -1.0
                && e.spell <= i16::MAX as u32
                && e.spec.power >= spec.minimum_power
                && e.spec.power <= spec.maximum_power
                && spec.school.is_none_or(|s| s == e.school)
                && if e.spec.beneficial {
                    spec.beneficial
                } else {
                    spec.harmful
                }
        };
        let count = self.entries.iter().filter(|e| eligible(e)).count();
        if count > out.capacity() - out.len() {
            return Err(RegistryError::OutputCapacity);
        }
        for entry in self.entries.iter().filter(|e| eligible(e)) {
            out.push((entry.spell, entry.spec.layer));
        }
        Ok(())
    }
    pub fn remove(&mut self, selected: &[(u32, u16)]) -> Result<(), RegistryError> {
        if selected.len() > self.capacity {
            return Err(RegistryError::Capacity);
        }
        for (index, key) in selected.iter().enumerate() {
            if selected[..index].contains(key)
                || !self.entries.iter().any(|e| (e.spell, e.spec.layer) == *key)
            {
                return Err(RegistryError::MissingEntry);
            }
        }
        if selected.is_empty() {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(RegistryError::RevisionExhausted)?;
        self.entries
            .retain(|e| !selected.contains(&(e.spell, e.spec.layer)));
        self.revision = revision;
        Ok(())
    }
}

fn validate_entry(entry: &EnchantmentEntry) -> Result<(), RegistryError> {
    let metadata = entry.metadata;
    if entry.spell == 0
        || entry.spell > u32::from(u16::MAX)
        || !entry.start_time.is_finite()
        || !entry.spec.duration.is_finite()
        || entry.spec.duration < 0.0 && entry.spec.duration != -1.0
        || !entry.spec.value.is_finite()
        || !metadata.degrade_modifier.is_finite()
        || !metadata.degrade_limit.is_finite()
        || !metadata.last_time_degraded.is_finite()
    {
        return Err(RegistryError::InvalidEntry);
    }
    Ok(())
}

impl EnchantmentRegistry {
    /// ACE StartCooldown uses a shared synthetic spell ID and fixed layer one;
    /// it does not compete with other cooldown IDs in the common 0x8000 category.
    pub fn propose_cooldown(
        &self,
        group: u16,
        device: u32,
        duration: f64,
    ) -> Result<PreparedEnchantment, RegistryError> {
        if group >= 0x8000
            || device == 0
            || !duration.is_finite()
            || !(0.0..=86400.0).contains(&duration)
        {
            return Err(RegistryError::InvalidEntry);
        }
        let spell = u32::from(0x8000 | group);
        if self.entries.iter().any(|e| e.spell == spell) {
            return Err(RegistryError::InvalidEntry);
        }
        if self.entries.len() >= self.capacity {
            return Err(RegistryError::Capacity);
        }
        let entry = EnchantmentEntry {
            spell,
            caster: device,
            school: MagicSchool::Item,
            spec: EnchantmentSpec {
                category: 0x8000,
                power: 0,
                duration,
                layer: 1,
                stat_type: 0x1000000,
                stat_key: 0,
                value: 0.0,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: EnchantmentMetadata {
                enchantment_category: 8,
                has_spell_set_id: true,
                degrade_limit: -666.0,
                ..Default::default()
            },
        };
        validate_entry(&entry)?;
        Ok(PreparedEnchantment {
            before_revision: self.revision,
            replace: None,
            application: EnchantmentApplication {
                entry,
                stack: EnchantmentStack::Initial,
                revision: self
                    .revision
                    .checked_add(1)
                    .ok_or(RegistryError::RevisionExhausted)?,
            },
        })
    }
    pub fn preview(
        &self,
        proposal: &PreparedEnchantment,
    ) -> Result<Vec<EnchantmentEntry>, RegistryError> {
        if self.revision != proposal.before_revision {
            return Err(RegistryError::RevisionExhausted);
        }
        let mut entries = self.entries.clone();
        if let Some(index) = proposal.replace {
            entries[index] = proposal.application.entry.clone();
        } else {
            entries.push(proposal.application.entry.clone());
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod restoration_tests;

#[cfg(test)]
#[path = "enchantment_allocation_tests.rs"]
mod allocation_tests;
