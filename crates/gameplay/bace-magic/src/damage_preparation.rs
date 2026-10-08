//! GDLE SpellProjectile/WeenieObject property preparation over accepted native
//! content. Runtime owns DAT/equipment identity resolution, never these defaults.
use crate::{EffectError as E, MagicDamageProfile, MagicWand};
use bace_content::WeenieV1;
use bace_gameplay_api::weapon_combat::{PhysicalRatings, PhysicalSkill};
pub struct MagicDamageEquipment<'a> {
    pub entity: u32,
    pub revision: u64,
    pub location: u32,
    pub level: Option<u32>,
    pub weenie: &'a WeenieV1,
}
pub struct MagicDamagePreparation<'a> {
    pub player: bool,
    pub weenie: &'a WeenieV1,
    pub equipment: &'a [MagicDamageEquipment<'a>],
    pub skills: &'a [(u32, PhysicalSkill)],
    pub base_attributes: [u32; 6],
    pub base_shield_skill: u32,
}
fn int(w: &WeenieV1, id: u32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.value)
}
fn float(w: &WeenieV1, id: u32, default: f64) -> f64 {
    w.properties
        .floats
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn boolean(w: &WeenieV1, id: u32) -> bool {
    w.properties.bools.iter().any(|p| p.id == id && p.value)
}
fn imbues(w: &WeenieV1) -> u32 {
    [179, 303, 304, 305, 306]
        .into_iter()
        .fold(0, |v, id| v | int(w, id) as u32)
}
/// GDLE MagicSystem::DeterminePowerLevelOfComponent / SpellFormula's first slot.
/// Unknown and absent components explicitly return the source's zero level.
pub fn spell_formula_level(formula: &[u32]) -> u32 {
    match formula.first().copied().unwrap_or(0) {
        n @ 1..=6 => n,
        0x6e => 6,
        0x70 | 0xc0 => 7,
        0xc1 => 8,
        _ => 0,
    }
}
/// GDLE SpellProjectile constructor intensity used by Launch/Explode scripts.
pub fn spell_projectile_intensity(formula_level: u32) -> f32 {
    ((f64::from(formula_level) - 1.0) / 15.0).clamp(0.0, 1.0) as f32
}
/// Source GetRating's exact constituent IDs. These are raw owner inputs, later
/// recomposed using live registry modifiers at an authoritative impact boundary.
pub fn magic_rating_ids() -> &'static [u32] {
    &[
        307, 370, 309, 333, 329, 314, 374, 299, 335, 381, 383, 308, 371, 310, 334, 316, 375, 336,
        382, 384, 357, 356, 323, 376, 342, 317, 351,
    ]
}
pub fn magic_ratings(properties: &[(u32, i32)]) -> Result<PhysicalRatings, E> {
    let sum = |ids: &[u32]| -> Result<i32, E> {
        ids.iter().try_fold(0i32, |v, id| {
            v.checked_add(properties.iter().find(|p| p.0 == *id).map_or(0, |p| p.1))
                .ok_or(E::Overflow)
        })
    };
    Ok(PhysicalRatings {
        damage: sum(&[307, 370, 309, 333])?,
        weakness: sum(&[329])?,
        critical_damage: sum(&[314, 374, 299, 335])?,
        pk_damage: sum(&[381, 383])?,
        resistance: sum(&[308, 371, 310, 334])?,
        critical_resistance: sum(&[316, 375, 336])?,
        pk_resistance: sum(&[382, 384])?,
        reckless: sum(&[357])?,
        sneak: sum(&[356])?,
    })
}
pub fn prepare_magic_damage_profile(
    input: MagicDamagePreparation<'_>,
) -> Result<MagicDamageProfile, E> {
    if input.equipment.len() > 64 || input.skills.len() > 64 {
        return Err(E::Capacity);
    }
    let w = input.weenie;
    let mut p = MagicDamageProfile::neutral(input.player);
    let nonnegative = |v: i32| u32::try_from(v).map_err(|_| E::InvalidState);
    p.spellcraft = input
        .weenie
        .properties
        .ints
        .iter()
        .find(|p| p.id == 106)
        .map(|p| nonnegative(p.value))
        .transpose()?;
    let skill = |id| {
        input.skills.iter().find(|v| v.0 == id).map_or(
            PhysicalSkill {
                advancement: 0,
                current: 0,
            },
            |v| v.1,
        )
    };
    p.current_enemy = w
        .properties
        .instance_ids
        .iter()
        .find(|p| p.id == 9)
        .map(|p| p.value)
        .filter(|v| *v != 0);
    p.creature_type = nonnegative(int(w, 2))?;
    p.base_strength = input.base_attributes[0];
    p.base_endurance = input.base_attributes[1];
    p.elemental_modifier = float(w, 152, 0.);
    p.magic_defense = skill(15);
    p.sneak = skill(51);
    p.deception = skill(20);
    p.assess_person = skill(19);
    p.critical_defense = int(w, 233) > 0;
    p.augmentation_family = int(w, 239) > 0;
    p.ignore_magic_resistance = boolean(w, 65);
    for (index, (id, aug)) in [
        (64, 240),
        (65, 241),
        (66, 242),
        (67, 244),
        (68, 245),
        (69, 243),
        (70, 246),
        (166, 327),
        (125, 0),
        (72, 0),
        (74, 0),
    ]
    .into_iter()
    .enumerate()
    {
        p.resistances[index].quality.raw = float(w, id, 1.0);
        p.resistances[index].augmentation = if aug == 0 {
            0
        } else {
            nonnegative(int(w, aug))?
        };
    }
    p.rating_properties = magic_rating_ids()
        .iter()
        .map(|id| (*id, int(w, *id)))
        .collect();
    p.ratings = magic_ratings(&p.rating_properties)?;
    p.healing_ratings = [
        int(w, 323)
            .checked_add(int(w, 376))
            .and_then(|v| v.checked_add(int(w, 342)))
            .ok_or(E::Overflow)?,
        int(w, 317),
        int(w, 351),
    ];
    p.boost_resistances = [float(w, 71, 1.), float(w, 73, 1.), float(w, 75, 1.)];
    p.dot_ratings = [nonnegative(int(w, 310))?, nonnegative(int(w, 350))?];
    for (index, e) in input.equipment.iter().enumerate() {
        if e.entity == 0
            || e.location == 0
            || input.equipment[..index]
                .iter()
                .any(|old| old.entity == e.entity)
        {
            return Err(E::InvalidState);
        }
        let optional_int = |id| {
            e.weenie
                .properties
                .ints
                .iter()
                .find(|v| v.id == id)
                .map(|p| nonnegative(p.value))
                .transpose()
        };
        let mut item_skills = [0; 5];
        for (index, id) in [34, 33, 31, 32, 43].into_iter().enumerate() {
            if let Some(skill) = e.weenie.properties.skills.iter().find(|s| s.id == id) {
                item_skills[index] = skill
                    .value
                    .init_level
                    .checked_add(u32::from(skill.value.level_from_pp))
                    .ok_or(E::Overflow)?;
            }
        }
        p.proc_items.push(crate::MagicProcItem {
            item: e.entity,
            spellcraft: optional_int(106)?,
            skills: item_skills,
            cloak: e.location & 0x08000000 != 0,
            wield_difficulty: optional_int(160)?,
        });
        if e.location & 0x08000000 != 0 {
            let effect = match int(e.weenie, 352) {
                2 => Some(crate::MagicCloakEffect::Absorb),
                1 => e
                    .weenie
                    .properties
                    .data_ids
                    .iter()
                    .find(|p| p.id == 55)
                    .map(|p| crate::MagicCloakEffect::Spell(p.value)),
                _ => None,
            };
            if let Some(effect) = effect {
                if p.cloak.is_some() {
                    return Err(E::InvalidState);
                }
                p.cloak = Some(crate::MagicCloak {
                    item: e.entity,
                    level: e.level.ok_or(E::InvalidState)?,
                    effect,
                });
            }
        }
        if e.location & 0x70000000 != 0 {
            let slot = match e.location & 0x70000000 {
                0x10000000 => 0,
                0x20000000 => 1,
                0x40000000 => 2,
                _ => return Err(E::InvalidState),
            };
            let spell = e
                .weenie
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == 55)
                .ok_or(E::InvalidState)?
                .value;
            p.sigils.push(crate::MagicSigil {
                item: e.entity,
                slot,
                level: e.level.ok_or(E::InvalidState)?,
                spell,
            });
        }
        if e.location & 0x01000000 != 0 && int(e.weenie, 1) & 0x8000 != 0 {
            if p.wand.is_some() {
                return Err(E::InvalidState);
            }
            p.wand = Some(prepare_magic_wand(e.entity, e.revision, e.weenie)?);
        }
        if int(e.weenie, 51) == 2 && e.location & 0x00400000 != 0 {
            p.missile_absorption = imbues(e.weenie) & 0x20000000 != 0;
        }
        if int(e.weenie, 51) == 4 && e.location & 0x00200000 != 0 {
            if p.shield.is_some() {
                return Err(E::InvalidState);
            }
            p.shield_entity = Some(e.entity);
            p.shield = Some((
                PhysicalSkill {
                    advancement: skill(48).advancement,
                    current: input.base_shield_skill,
                },
                float(e.weenie, 159, 0.) as f32,
            ));
        }
    }
    crate::validate_magic_damage_profile(&p)?;
    Ok(p)
}

/// Current object qualities for the source's cached wand identity. Also accepts
/// an object whose type changed after launch: GDLE looks it up by identity.
pub fn prepare_magic_wand(entity: u32, revision: u64, weenie: &WeenieV1) -> Result<MagicWand, E> {
    prepare_magic_wand_properties(entity, revision, &weenie.properties)
}
/// Typed scalar projection for an already-owned object's accepted mutation.
pub fn prepare_magic_wand_properties(
    entity: u32,
    revision: u64,
    properties: &bace_content::SparseProperties,
) -> Result<MagicWand, E> {
    let integer = |id| {
        properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.value)
    };
    let number = |id, default| {
        properties
            .floats
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let flag = |id| properties.bools.iter().any(|p| p.id == id && p.value);
    let nonnegative = |v: i32| u32::try_from(v).map_err(|_| E::InvalidState);
    let wand = MagicWand {
        entity,
        revision,
        damage_type: nonnegative(integer(45))?,
        elemental_modifier: number(152, 0.),
        elemental_present: properties.floats.iter().any(|p| p.id == 152),
        inherit_wielder: integer(36) < 9999,
        imbues: [179, 303, 304, 305, 306]
            .into_iter()
            .fold(0, |bits, id| bits | integer(id) as u32),
        biting: number(147, 0.),
        double_enchant_biting: properties.floats.iter().any(|p| p.id == 147),
        crushing: number(136, 0.),
        double_enchant_crushing: properties.floats.iter().any(|p| p.id == 136),
        slayer_type: nonnegative(integer(166))?,
        slayer_bonus: number(138, 0.),
        resistance_cleaving: properties
            .ints
            .iter()
            .find(|v| v.id == 263)
            .map(|v| u32::try_from(v.value).map_err(|_| E::InvalidState))
            .transpose()?,
        ignore_magic_resistance: flag(65),
    };
    let mut validation = MagicDamageProfile::neutral(false);
    validation.wand = Some(wand.clone());
    crate::validate_magic_damage_profile(&validation)?;
    Ok(wand)
}
