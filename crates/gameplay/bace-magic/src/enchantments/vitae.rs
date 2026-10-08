//! ACE EnchantmentManager.UpdateVitae/ReduceVitae/RemoveVitae registry mutation.
//! Progression owns CP thresholds; this owner preserves the exact supplied f32.
use super::*;
#[derive(Clone, Debug, PartialEq)]
pub struct VitaeMutation {
    before_revision: u64,
    after_revision: u64,
    entries: Vec<EnchantmentEntry>,
    entry: Option<EnchantmentEntry>,
    remove_after: Option<f64>,
}
impl VitaeMutation {
    pub fn before_revision(&self) -> u64 {
        self.before_revision
    }
    pub fn after_revision(&self) -> u64 {
        self.after_revision
    }
    pub fn entries(&self) -> &[EnchantmentEntry] {
        &self.entries
    }
    pub fn entry(&self) -> Option<&EnchantmentEntry> {
        self.entry.as_ref()
    }
    pub fn remove_after_seconds(&self) -> Option<f64> {
        self.remove_after
    }
}
impl EnchantmentRegistry {
    pub fn propose_vitae(
        &self,
        actor: u32,
        template: Option<&EnchantmentEntry>,
        value: Option<f32>,
        remove_after: Option<f64>,
    ) -> Result<VitaeMutation, RegistryError> {
        if actor == 0
            || value.is_some_and(|v| !v.is_finite() || !(0.0..=1.01).contains(&v))
            || remove_after.is_some_and(|t| t != 2.0)
            || value.is_none() && remove_after.is_some()
        {
            return Err(RegistryError::InvalidEntry);
        }
        if self.entries.iter().filter(|e| e.spell == 666).count() > 1 {
            return Err(RegistryError::InvalidEntry);
        }
        let existing = self.entries.iter().find(|e| e.spell == 666);
        let entry = if let Some(value) = value {
            let mut entry = existing
                .or(template)
                .ok_or(RegistryError::MissingEntry)?
                .clone();
            if entry.spell != 666 {
                return Err(RegistryError::InvalidEntry);
            }
            entry.spec.value = value;
            if existing.is_none() {
                entry.caster = actor;
                entry.spec.layer = 1;
                entry.start_time = 0.0;
                entry.metadata.enchantment_category = 4;
            }
            validate_entry(&entry)?;
            Some(entry)
        } else {
            None
        };
        let mut entries = self.entries.clone();
        if let Some(index) = entries.iter().position(|e| e.spell == 666) {
            if let Some(entry) = &entry {
                entries[index] = entry.clone();
            } else {
                entries.remove(index);
            }
        } else if let Some(entry) = &entry {
            entries.push(entry.clone());
        }
        if entries.len() > self.capacity {
            return Err(RegistryError::Capacity);
        }
        let after_revision = if existing.is_some() || value.is_some() {
            self.revision
                .checked_add(1)
                .ok_or(RegistryError::RevisionExhausted)?
        } else {
            self.revision
        };
        Ok(VitaeMutation {
            before_revision: self.revision,
            after_revision,
            entries,
            entry,
            remove_after,
        })
    }
    pub fn adopt_vitae(
        &mut self,
        mutation: VitaeMutation,
    ) -> Result<Option<EnchantmentEntry>, RegistryError> {
        if self.revision != mutation.before_revision {
            return Err(RegistryError::RevisionExhausted);
        }
        self.entries = mutation.entries;
        self.revision = mutation.after_revision;
        Ok(mutation.entry)
    }
}
