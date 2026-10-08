//! Cold SentinelCommands/Player_Spells buff preparation from verified DAT and
//! accepted server rows. Buff list provenance: official ACE 47edade3, AGPL-3.0-only.
use bace_content::SpellRowV1;
use bace_dat::SpellTable;
use bace_gameplay_api::staff::{StaffBuffPlan, StaffSpellDefinition};
use bace_magic::{EnchantmentEntry, EnchantmentMetadata, EnchantmentSpec, MagicSchool};
use std::collections::{BTreeMap, BTreeSet};
const BUFFS: &[&str] = &[
    "Strength",
    "Invulnerability",
    "FireProtection",
    "Armor",
    "Rejuvenation",
    "Regeneration",
    "ManaRenewal",
    "Impregnability",
    "MagicResistance",
    "LightWeaponsMastery",
    "FinesseWeaponsMastery",
    "HeavyWeaponsMastery",
    "MissileWeaponsMastery",
    "AcidProtection",
    "CreatureEnchantmentMastery",
    "ItemEnchantmentMastery",
    "LifeMagicMastery",
    "WarMagicMastery",
    "ManaMastery",
    "ArcaneEnlightenment",
    "ArcanumSalvaging",
    "ArmorExpertise",
    "ItemExpertise",
    "MagicItemExpertise",
    "WeaponExpertise",
    "MonsterAttunement",
    "PersonAttunement",
    "DeceptionMastery",
    "HealingMastery",
    "LeadershipMastery",
    "LockpickMastery",
    "Fealty",
    "JumpingMastery",
    "Sprint",
    "BludgeonProtection",
    "ColdProtection",
    "LightningProtection",
    "BladeProtection",
    "PiercingProtection",
    "Endurance",
    "Coordination",
    "Quickness",
    "Focus",
    "Willpower",
    "CookingMastery",
    "FletchingMastery",
    "AlchemyMastery",
    "VoidMagicMastery",
    "SummoningMastery",
    "SwiftKiller",
    "Defender",
    "BloodDrinker",
    "HeartSeeker",
    "HermeticLink",
    "SpiritDrinker",
    "DualWieldMastery",
    "TwoHandedMastery",
    "DirtyFightingMastery",
    "RecklessnessMastery",
    "SneakAttackMastery",
    "ShieldMastery",
    "@Impenetrability",
    "@PiercingBane",
    "@BludgeonBane",
    "@BladeBane",
    "@AcidBane",
    "@FlameBane",
    "@FrostBane",
    "@LightningBane",
];
#[derive(Clone)]
pub struct PreparedStaffMagicAssets {
    pub definitions: Vec<StaffSpellDefinition>,
    pub plans: Vec<StaffBuffPlan>,
    /// Direct effects for the staff-only index; never usable as player casting assets.
    pub enchantments: Vec<EnchantmentEntry>,
}
pub fn prepare_staff_magic_assets(
    table: &SpellTable,
    rows: &BTreeMap<u32, SpellRowV1>,
) -> Result<PreparedStaffMagicAssets, String> {
    if table.spells.len() > 8192 || rows.len() > 16384 {
        return Err("staff spell assets capacity".into());
    }
    let names =
        bace_loot::ace_tables::enum_members("SpellId").ok_or("missing pinned spell enum")?;
    let by_id: BTreeMap<_, _> = names
        .iter()
        .filter_map(|&(name, id)| u32::try_from(id).ok().map(|id| (id, name)))
        .collect();
    let definitions = table
        .spells
        .iter()
        .filter(|(id, _)| rows.contains_key(id))
        .map(|(&id, spell)| {
            if id == 0 || id > 65535 || spell.name.len() > 1024 {
                return Err("invalid staff spell definition".to_string());
            }
            Ok(StaffSpellDefinition {
                spell: id,
                name: spell.name.clone(),
                enum_name: by_id.get(&id).copied().unwrap_or("").to_string(),
                targeted: spell.non_component_target_type != 0,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let resolve = |name: &str| {
        bace_loot::ace_tables::enum_value("SpellId", name)
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| table.spells.contains_key(id) && rows.contains_key(id))
    };
    let mut plans = Vec::new();
    let mut required = BTreeSet::new();
    for requested in 1..=8 {
        let level = if requested == 8 && resolve("ArmorOther8").is_none() {
            7
        } else {
            requested
        };
        let mut plan = StaffBuffPlan {
            level: requested,
            self_spells: vec![],
            other_spells: vec![],
            banes: vec![],
            effects: vec![],
            missing: vec![],
        };
        for &prefix in BUFFS {
            if let Some(bane) = prefix.strip_prefix('@') {
                let name = format!("{bane}{level}");
                if let Some(id) = resolve(&name) {
                    plan.banes.push(id);
                    required.insert(id);
                } else {
                    plan.missing.push(name);
                }
            } else {
                for (branch, alternate, output) in [
                    ("Self", "Other", &mut plan.self_spells),
                    ("Other", "Self", &mut plan.other_spells),
                ] {
                    let name = format!("{prefix}{branch}{level}");
                    let fallback = format!("{prefix}{alternate}{level}");
                    if let Some(id) = resolve(&name).or_else(|| resolve(&fallback)) {
                        output.push(id);
                        required.insert(id);
                        let effect = table.spells[&id].target_effect;
                        if !plan.effects.contains(&(id, effect)) {
                            plan.effects.push((id, effect));
                        }
                    } else {
                        plan.missing.push(name);
                    }
                }
            }
        }
        // The original server applies life, creature, then item effects, each retaining list order.
        plan.self_spells.sort_by_key(|id| table.spells[id].school);
        plan.other_spells.sort_by_key(|id| table.spells[id].school);
        plans.push(plan);
    }
    if resolve("SentinelRun").is_some() {
        required.insert(1644);
    }
    let mut enchantments = Vec::new();
    for id in required {
        let base = &table.spells[&id];
        let row = &rows[&id];
        if row.id != id || base.meta_type != 1 {
            return Err(format!("unsupported staff buff shape {id}"));
        }
        let school = match base.school {
            2 => MagicSchool::Life,
            3 => MagicSchool::Item,
            4 => MagicSchool::Creature,
            _ => return Err(format!("invalid staff buff school {id}")),
        };
        let (duration, degrade_modifier, degrade_limit) = base
            .enchantment
            .ok_or("missing staff enchantment metadata")?;
        let beneficial = base.flags & 4 != 0;
        let spec = EnchantmentSpec {
            category: u16::try_from(base.category)
                .map_err(|_| "staff enchantment category overflow")?,
            power: base.power,
            duration,
            layer: 0,
            stat_type: row.stat_mod_type.unwrap_or(0) | if beneficial { 0x02000000 } else { 0 },
            stat_key: row.stat_mod_key.unwrap_or(0),
            value: row.stat_mod_val.unwrap_or(0.),
            beneficial,
            set_id: None,
        };
        let metadata = EnchantmentMetadata {
            enchantment_category: base.meta_type,
            has_spell_set_id: false,
            degrade_modifier,
            degrade_limit,
            last_time_degraded: 0.,
            spell_set_id: 0,
        };
        let definition = crate::player_assets::player_enchantment_definition(id, table, rows)
            .ok_or("missing staff enchantment definition")?;
        enchantments.push(EnchantmentEntry {
            spell: id,
            caster: 0,
            school,
            spec,
            start_time: 0.,
            is_set_spell: definition.is_set_spell,
            is_level8_aura: definition.is_level8_aura,
            metadata,
        });
    }
    Ok(PreparedStaffMagicAssets {
        definitions,
        plans,
        enchantments,
    })
}
/// Pinned Enum.TryParse(ignoreCase) boundary for addspell/removespell, bounded.
pub fn resolve_staff_spell_name(
    input: &str,
    definitions: &[StaffSpellDefinition],
    require_defined: bool,
) -> Option<u32> {
    if input.len() > 128 || definitions.len() > 8192 {
        return None;
    }
    let parsed = input.parse::<u32>().ok().or_else(|| {
        definitions
            .iter()
            .find(|d| d.enum_name.eq_ignore_ascii_case(input))
            .map(|d| d.spell)
    })?;
    if require_defined
        && !definitions
            .iter()
            .any(|d| d.spell == parsed && !d.enum_name.is_empty())
    {
        None
    } else {
        Some(parsed)
    }
}
