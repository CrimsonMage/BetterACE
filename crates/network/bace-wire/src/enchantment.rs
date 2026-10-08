//! ACE Network.Structure.Enchantment/EnchantmentRegistry/LayeredSpell, pinned
//! 47edade3bd3f6044b676d4eb877c4965c7eda62b, AGPL-3.0-only, ACE contributors.
use crate::{WireError, Writer};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Enchantment {
    pub spell_id: u16,
    pub layer: u16,
    pub category: u16,
    pub has_spell_set_id: u16,
    pub power: u32,
    pub start_time: f64,
    pub duration: f64,
    pub caster_id: u32,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    pub last_time_degraded: f64,
    pub stat_type: u32,
    pub stat_key: u32,
    pub stat_value: f32,
    pub spell_set_id: Option<u32>,
}
impl Enchantment {
    pub(crate) fn write(&self, w: &mut Writer) -> Result<(), WireError> {
        if (self.has_spell_set_id != 0) != self.spell_set_id.is_some() {
            return Err(WireError::InvalidEncoding);
        }
        w.u16(self.spell_id);
        w.u16(if self.spell_id == 666 { 0 } else { self.layer });
        w.u16(self.category);
        w.u16(self.has_spell_set_id);
        w.u32(self.power);
        w.f64(self.start_time);
        w.f64(self.duration);
        w.u32(self.caster_id);
        w.f32(self.degrade_modifier);
        w.f32(self.degrade_limit);
        w.f64(self.last_time_degraded);
        w.u32(self.stat_type);
        w.u32(self.stat_key);
        w.f32(self.stat_value);
        if let Some(id) = self.spell_set_id {
            w.u32(id);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        let mut w = Writer::default();
        self.write(&mut w)?;
        Ok(w.into_bytes())
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnchantmentRegistry {
    pub multiplicative: Vec<Enchantment>,
    pub additive: Vec<Enchantment>,
    pub cooldown: Vec<Enchantment>,
    pub vitae: Option<Enchantment>,
}
impl EnchantmentRegistry {
    pub fn count(&self) -> usize {
        self.multiplicative.len()
            + self.additive.len()
            + self.cooldown.len()
            + usize::from(self.vitae.is_some())
    }
    pub(crate) fn write(&self, w: &mut Writer, max_entries: usize) -> Result<(), WireError> {
        let count = self
            .multiplicative
            .len()
            .checked_add(self.additive.len())
            .and_then(|n| n.checked_add(self.cooldown.len()))
            .and_then(|n| n.checked_add(usize::from(self.vitae.is_some())))
            .ok_or(WireError::LimitExceeded)?;
        if count > max_entries {
            return Err(WireError::LimitExceeded);
        }
        let mask = u32::from(!self.multiplicative.is_empty())
            | u32::from(!self.additive.is_empty()) << 1
            | u32::from(self.vitae.is_some()) << 2
            | u32::from(!self.cooldown.is_empty()) << 3;
        w.u32(mask);
        for entries in [&self.multiplicative, &self.additive, &self.cooldown] {
            if !entries.is_empty() {
                write_enchantments(w, entries, max_entries)?;
            }
        }
        if let Some(vitae) = self.vitae {
            if vitae.spell_id != 666 {
                return Err(WireError::InvalidEncoding);
            }
            vitae.write(w)?;
        }
        Ok(())
    }
    pub fn encode(&self, max_entries: usize, max_bytes: usize) -> Result<Vec<u8>, WireError> {
        let mut w = Writer::default();
        self.write(&mut w, max_entries)?;
        let bytes = w.into_bytes();
        if bytes.len() > max_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(bytes)
    }
}
pub(crate) fn write_enchantments(
    w: &mut Writer,
    entries: &[Enchantment],
    max_entries: usize,
) -> Result<(), WireError> {
    if entries.len() > max_entries || entries.len() > u32::MAX as usize {
        return Err(WireError::LimitExceeded);
    }
    w.u32(entries.len() as u32);
    for entry in entries {
        entry.write(w)?;
    }
    Ok(())
}
