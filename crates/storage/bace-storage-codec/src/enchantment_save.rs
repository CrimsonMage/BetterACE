//! Frozen registry entry V1, derived from pinned ACE PropertiesEnchantmentRegistry.
//! Runtime spell/enchantment structs must never be serialized in its place.
use crate::SaveCodecError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenEnchantmentV1 {
    pub schema_version: u16,
    pub enchantment_category: u32,
    pub spell_id: i32,
    pub layer_id: u16,
    pub has_spell_set_id: bool,
    pub spell_category: u32,
    pub power_level: u32,
    /// ACE registry-relative start value, not a client clock or Unix timestamp.
    pub start_time: f64,
    /// Seconds, or exactly -1 for a non-expiring equipped enchantment.
    pub duration: f64,
    pub caster_object_id: u32,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    pub last_time_degraded: f64,
    pub stat_mod_type: u32,
    pub stat_mod_key: u32,
    pub stat_mod_value: f32,
    pub spell_set_id: i32,
}
impl FrozenEnchantmentV1 {
    pub fn validate(&self) -> Result<(), SaveCodecError> {
        if self.schema_version != 1 {
            return Err(SaveCodecError::Invalid("unsupported enchantment schema"));
        }
        if self.spell_id <= 0
            || self.spell_id > i32::from(u16::MAX)
            || self.spell_category > u32::from(u16::MAX)
        {
            return Err(SaveCodecError::Invalid(
                "enchantment identity cannot be represented on wire",
            ));
        }
        if !self.start_time.is_finite()
            || !self.duration.is_finite()
            || self.duration < 0.0 && self.duration != -1.0
            || !self.last_time_degraded.is_finite()
            || [
                self.degrade_modifier,
                self.degrade_limit,
                self.stat_mod_value,
            ]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(SaveCodecError::Invalid(
                "nonfinite or invalid enchantment time/modifier",
            ));
        }
        Ok(())
    }
}
pub fn validate_enchantments_v1(entries: &[FrozenEnchantmentV1]) -> Result<(), SaveCodecError> {
    if entries.len() > 4096 {
        return Err(SaveCodecError::Invalid("enchantment registry count"));
    }
    let mut keys = std::collections::BTreeSet::new();
    for entry in entries {
        entry.validate()?;
        if !keys.insert((entry.spell_id, entry.layer_id)) {
            return Err(SaveCodecError::Invalid("duplicate enchantment layer"));
        }
    }
    Ok(())
}
