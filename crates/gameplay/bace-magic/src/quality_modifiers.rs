//! ACE PropertiesEnchantmentRegistryExtensions.GetEnchantmentsTopLayerByStatModType
//! (type+key, handleMultiple=true). Filter before selecting category winners;
//! preserve first category occurrence and stable exact ties.
use crate::{EnchantmentEntry, EnchantmentRegistry};
pub fn enchantment_modifiers(
    registry: &EnchantmentRegistry,
    flags: u32,
    key: u32,
) -> Vec<&EnchantmentEntry> {
    let single = flags | 0x1000;
    let multiple = flags | 0x2000;
    let mut selected: Vec<&EnchantmentEntry> = Vec::new();
    for entry in registry.entries().iter().filter(|e| {
        (e.spec.stat_type & single) == single && e.spec.stat_key == key
            || (e.spec.stat_type & multiple) == multiple
                && e.spec.stat_type & 0x800000 == 0
                && e.spec.stat_key == 0
    }) {
        if let Some(index) = selected
            .iter()
            .position(|old| old.spec.category == entry.spec.category)
        {
            let old = selected[index];
            let tie = |e: &EnchantmentEntry| {
                if e.is_set_spell {
                    f64::from(e.spell)
                } else {
                    e.start_time
                }
            };
            if entry
                .spec
                .power
                .cmp(&old.spec.power)
                .then(entry.is_level8_aura.cmp(&old.is_level8_aura))
                .then_with(|| tie(entry).total_cmp(&tie(old)))
                .is_gt()
            {
                selected[index] = entry;
            }
        } else {
            selected.push(entry);
        }
    }
    selected
}
