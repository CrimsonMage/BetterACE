//! Cold projection of accepted registries and verified DAT enchantability filter.
use bace_combat::preparation::{PhysicalQualityFamily as F, PhysicalQualityProjection};
use bace_content::WeenieV1;
use bace_dat::QualityFilter;
use bace_magic::{EnchantmentRegistry, enchant_physical_quality};
use bace_types::EntityId;
pub fn prepare_player_physical_qualities(
    actor: EntityId,
    source: &WeenieV1,
    player_registry: &EnchantmentRegistry,
    equipment: &[(EntityId, &WeenieV1, &EnchantmentRegistry)],
    filter: &QualityFilter,
) -> Result<Vec<PhysicalQualityProjection>, String> {
    if actor.0 == 0 || equipment.len() > 128 {
        return Err("physical enchantment identities/capacity".into());
    }
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::from([actor]);
    append(actor, source, player_registry, filter, &mut result)?;
    for &(item, source, registry) in equipment {
        if item.0 == 0 || !seen.insert(item) {
            return Err("duplicate physical enchantment identity".into());
        }
        append(item, source, registry, filter, &mut result)?;
    }
    Ok(result)
}
fn append(
    actor: EntityId,
    source: &WeenieV1,
    registry: &EnchantmentRegistry,
    filter: &QualityFilter,
    result: &mut Vec<PhysicalQualityProjection>,
) -> Result<(), String> {
    let mut ints = std::collections::BTreeSet::from([
        28, 44, 49, 56, 299, 307, 308, 309, 310, 314, 316, 329, 333, 334, 335, 336, 356, 357, 360,
        361, 370, 371, 374, 375, 381, 382, 383, 384,
    ]);
    ints.extend(source.properties.ints.iter().map(|p| p.id));
    let mut floats = std::collections::BTreeSet::from([
        13, 14, 15, 16, 17, 18, 19, 62, 64, 65, 66, 67, 68, 69, 70, 136, 138, 147, 151, 165, 166,
        168,
    ]);
    floats.extend(source.properties.floats.iter().map(|p| p.id));
    if ints.len() + floats.len() > 2048 || source.properties.body_parts.len() > 256 {
        return Err("physical enchantment property capacity".into());
    }
    for key in ints.into_iter().filter(|key| filter.allows_int(*key)) {
        let raw = f64::from(
            source
                .properties
                .ints
                .iter()
                .find(|p| p.id == key)
                .map_or(0, |p| p.value),
        );
        let (details, value) = enchant_physical_quality(registry, 4, key, raw, true)
            .map_err(|e| format!("physical integer enchantment: {e:?}"))?;
        result.push(PhysicalQualityProjection {
            entity: actor.0,
            family: F::Int,
            stat: key,
            part: None,
            details,
            value,
        });
    }
    for key in floats.into_iter().filter(|key| filter.allows_float(*key)) {
        let default = if [
            13, 14, 15, 16, 17, 18, 19, 64, 65, 66, 67, 68, 69, 70, 165, 166,
        ]
        .contains(&key)
        {
            1.
        } else {
            0.
        };
        let raw = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == key)
            .map_or(default, |p| p.value);
        let (details, value) = enchant_physical_quality(registry, 8, key, raw, false)
            .map_err(|e| format!("physical float enchantment: {e:?}"))?;
        result.push(PhysicalQualityProjection {
            entity: actor.0,
            family: F::Float,
            stat: key,
            part: None,
            details,
            value,
        });
    }
    for property in &source.properties.body_parts {
        let b = &property.value;
        let part = u32::try_from(property.id).map_err(|_| "negative body part")?;
        for (stat, raw) in [1, 2, 4, 8, 16, 32, 64, 1024].into_iter().zip([
            b.armor_vs_slash,
            b.armor_vs_pierce,
            b.armor_vs_bludgeon,
            b.armor_vs_cold,
            b.armor_vs_fire,
            b.armor_vs_acid,
            b.armor_vs_electric,
            b.armor_vs_nether,
        ]) {
            let details = bace_magic::enchant_body_quality(registry, stat, f64::from(raw))
                .map_err(|e| format!("physical body armor enchantment: {e:?}"))?;
            let value = f64::from(raw);
            result.push(PhysicalQualityProjection {
                entity: actor.0,
                family: F::BodyArmor,
                stat,
                part: Some(part),
                details,
                value,
            });
        }
    }
    if result.len() > 65536 {
        return Err("physical enchantment aggregate capacity".into());
    }
    Ok(())
}
/// Pinned registry tie metadata comes from the client spell/set table and matching
/// server row, never inferred from a persisted effect's caller-controlled flags.
pub fn player_enchantment_definition(
    id: u32,
    table: &bace_dat::SpellTable,
    rows: &std::collections::BTreeMap<u32, bace_content::SpellRowV1>,
) -> Option<crate::enchantment_saves::EnchantmentDefinition> {
    let spell = table.spells.get(&id)?;
    let row = rows.get(&id).filter(|row| row.id == id)?;
    let school = match spell.school {
        1 => bace_magic::MagicSchool::War,
        2 => bace_magic::MagicSchool::Life,
        3 => bace_magic::MagicSchool::Item,
        4 => bace_magic::MagicSchool::Creature,
        5 => bace_magic::MagicSchool::Void,
        _ => return None,
    };
    let (_, degrade_modifier, degrade_limit) = spell.enchantment?;
    let is_level8_aura = [
        "BloodDrinkerSelf8",
        "DefenderSelf8",
        "HeartSeekerSelf8",
        "SpiritDrinkerSelf8",
        "SwiftKillerSelf8",
        "HermeticLinkSelf8",
    ]
    .iter()
    .any(|name| bace_loot::ace_tables::enum_value("SpellId", name) == Some(i64::from(id)));
    let beneficial = spell.flags & 4 != 0;
    Some(crate::enchantment_saves::EnchantmentDefinition {
        school,
        is_set_spell: table
            .sets
            .values()
            .any(|tiers| tiers.values().any(|spells| spells.contains(&id))),
        is_level8_aura,
        category: u16::try_from(spell.category).ok()?,
        power: spell.power,
        degrade_modifier,
        degrade_limit,
        stat_type: row.stat_mod_type.unwrap_or(0) | if beneficial { 0x02000000 } else { 0 },
        stat_key: row.stat_mod_key.unwrap_or(0),
        beneficial,
    })
}
