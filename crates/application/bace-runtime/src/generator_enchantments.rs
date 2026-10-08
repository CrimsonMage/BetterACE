//! Pinned Creature_Magic.CreateItemSpell routing and permanent equipped entries.
//! Requires verified client spell metadata plus the matching native server row.
use bace_content::{SpellRowV1, WeenieV1};
use bace_dat::SpellTable;
use bace_magic::{EnchantmentEntry, EnchantmentMetadata, EnchantmentSpec, MagicSchool};
use bace_simulation::PreparedGeneratorEnchantment;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};

pub fn prepare_generator_item_enchantments(
    actor: EntityId,
    item: EntityId,
    template: &WeenieV1,
    spells: &SpellTable,
    rows: &BTreeMap<u32, SpellRowV1>,
) -> Result<Vec<PreparedGeneratorEnchantment>, String> {
    if actor.0 == 0 || item.0 == 0 || actor == item || template.properties.spell_book.len() > 256 {
        return Err("invalid generated item spell identities or capacity".into());
    }
    let mut prepared = Vec::new();
    let mut seen = BTreeSet::new();
    for known in &template.properties.spell_book {
        let id = u32::try_from(known.id).map_err(|_| "invalid generated item spell ID")?;
        if !seen.insert(id) {
            return Err("duplicate generated item spell".into());
        }
        let base = spells
            .spells
            .get(&id)
            .ok_or_else(|| format!("missing generated item spell {id}"))?;
        let school = match base.school {
            1 | 5 => continue, // CreateItemSpell has no War/Void case.
            2 => MagicSchool::Life,
            3 => MagicSchool::Item,
            4 => MagicSchool::Creature,
            _ => return Err(format!("invalid generated item spell school {id}")),
        };
        let row = rows
            .get(&id)
            .filter(|row| row.id == id)
            .ok_or_else(|| format!("missing generated item server spell {id}"))?;
        if base.meta_type != 1 {
            return Err(format!(
                "unsupported generated equipment spell effect {} for {id}",
                base.meta_type
            ));
        }
        if row.dot_duration.is_some_and(|duration| duration != 0.0)
            || base.flags & 0x10000 != 0
            || [617, 618, 630, 631, 636, 637, 638].contains(&base.category)
            || row
                .stat_mod_key
                .is_some_and(|key| [312, 318].contains(&key))
            || template
                .properties
                .data_ids
                .iter()
                .any(|property| property.id == 55 && property.value == id)
        {
            return Err(format!(
                "unsupported generated equipment periodic/proc spell {id}"
            ));
        }
        let (duration, degrade_modifier, degrade_limit) = base
            .enchantment
            .ok_or("missing item enchantment metadata")?;
        let beneficial = base.flags & 4 != 0;
        let stat_type = row.stat_mod_type.unwrap_or(0) | if beneficial { 0x02000000 } else { 0 };
        let set_id = template
            .properties
            .ints
            .iter()
            .find(|p| p.id == 265)
            .and_then(|p| u32::try_from(p.value).ok())
            .filter(|set| {
                spells
                    .sets
                    .get(set)
                    .is_some_and(|tiers| tiers.values().any(|ids| ids.contains(&id)))
            });
        let target = if school != MagicSchool::Item
            || [152, 154, 156, 158, 195, 695].contains(&base.category)
        {
            actor
        } else {
            item
        };
        let entry = EnchantmentEntry {
            spell: id,
            caster: item.0,
            school,
            spec: EnchantmentSpec {
                category: u16::try_from(base.category)
                    .map_err(|_| "item spell category overflow")?,
                power: base.power,
                duration,
                layer: 0,
                stat_type,
                stat_key: row.stat_mod_key.unwrap_or(0),
                value: row.stat_mod_val.unwrap_or(0.0),
                beneficial,
                set_id,
            },
            start_time: 0.0,
            is_set_spell: set_id.is_some(),
            is_level8_aura: false,
            metadata: EnchantmentMetadata {
                enchantment_category: base.meta_type,
                has_spell_set_id: set_id.is_some(),
                degrade_modifier,
                degrade_limit,
                last_time_degraded: 0.0,
                spell_set_id: set_id.map_or(0, |id| id as i32),
            },
        };
        // The same owner validates all numeric/range/stacking prerequisites before
        // admitting the creature. This isolated check never allocates a world owner.
        let mut registry =
            bace_magic::EnchantmentRegistry::new(1).map_err(|_| "item registry capacity")?;
        registry
            .add(entry.clone(), 0.0, true)
            .map_err(|_| format!("invalid generated item enchantment {id}"))?;
        prepared.push(PreparedGeneratorEnchantment { target, entry });
    }
    Ok(prepared)
}
