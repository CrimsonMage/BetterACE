//! ACE 47edade3 Network/Structure/EnchantmentRegistry and Enchantment writer.
use bace_gameplay_api::EnchantmentProjection;
use bace_wire::{Enchantment, EnchantmentRegistry, WireError};

/// Pure projection: cleanup (including recovered vitae) must be committed by
/// the owning domain before constructing a login description.
pub fn project_enchantments(
    entries: &[EnchantmentProjection],
    limit: usize,
) -> Result<EnchantmentRegistry, WireError> {
    if entries.len() > limit || entries.len() > 4096 {
        return Err(WireError::LimitExceeded);
    }
    let mut result = EnchantmentRegistry::default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.spell_id == 0
            || entries[..index]
                .iter()
                .any(|e| (e.spell_id, e.layer) == (entry.spell_id, entry.layer))
            || !entry.start_time.is_finite()
            || !entry.duration.is_finite()
            || entry.duration < 0.0 && entry.duration != -1.0
            || !entry.degrade_modifier.is_finite()
            || !entry.degrade_limit.is_finite()
            || !entry.last_time_degraded.is_finite()
            || !entry.stat_value.is_finite()
        {
            return Err(WireError::InvalidEncoding);
        }
        let wire = Enchantment {
            spell_id: entry.spell_id,
            layer: entry.layer,
            category: entry.category,
            has_spell_set_id: 1,
            power: entry.power,
            start_time: entry.start_time,
            duration: entry.duration,
            caster_id: entry.caster_id,
            degrade_modifier: entry.degrade_modifier,
            degrade_limit: entry.degrade_limit,
            last_time_degraded: entry.last_time_degraded,
            stat_type: entry.stat_type,
            stat_key: entry.stat_key,
            stat_value: entry.stat_value,
            spell_set_id: Some(entry.spell_set_id),
        };
        if entry.spell_id == 666 {
            if result.vitae.is_some()
                || entry.stat_value >= 1.0
                || (entry.stat_value - 1.0).abs() < 0.0001
            {
                return Err(WireError::InvalidEncoding);
            }
            result.vitae = Some(wire);
        } else if entry.spell_id > 0x8000 {
            result.cooldown.push(wire);
        } else if entry.stat_type & 0x4000 != 0 {
            result.multiplicative.push(wire);
        } else {
            result.additive.push(wire);
        }
    }
    Ok(result)
}
