//! Source property selection; arithmetic is owned by the character/magic domains.
use super::*;
use bace_character::{SkillBonuses, SkillValueInputs, VitalFormula};
use bace_gameplay_api::{
    AttributeId, ProgressionTarget, TraitDetails, VitalId, weapon_combat::PhysicalSkill,
};
use bace_simulation::{PreparedAttributeModifier, PreparedCharacterSkillInputs};
pub struct PlayerStatProjection {
    pub base_attributes: [u32; 6],
    pub current_attributes: [u32; 6],
    pub vitals: [bace_entity::VitalPool; 3],
    pub base_vitals: [u32; 3],
    pub skills: PreparedCharacterSkillInputs,
    pub physical_skills: Vec<(u32, PhysicalSkill)>,
}
fn formula(f: bace_dat::SkillFormula) -> VitalFormula {
    VitalFormula {
        enabled: f.x != 0,
        divisor: f.z,
        attribute1: f.attribute1,
        attribute2: f.attribute2,
    }
}
fn modifiers(
    r: &bace_magic::EnchantmentRegistry,
    flag: u32,
    key: u32,
) -> Result<(f32, i32), String> {
    let multiplier = bace_magic::enchantment_modifiers(r, flag | 0x4000, key)
        .iter()
        .fold(1f32, |v, e| v * e.spec.value);
    let additive = bace_magic::enchantment_modifiers(r, flag | 0x8000, key)
        .iter()
        .try_fold(0i32, |v, e| v.checked_add(e.spec.value as i32))
        .ok_or("enchantment modifier overflow")?;
    Ok((multiplier, additive))
}
pub fn prepare_player_stats(
    source: &bace_content::WeenieV1,
    state: &bace_simulation::OwnedPlayerState,
    dat: &PreparedAvatarDat,
    gear_health: u32,
) -> Result<PlayerStatProjection, String> {
    prepare_stats(
        source,
        &state.character.progression,
        state
            .enchantments
            .as_ref()
            .ok_or("player registry missing")?,
        dat,
        gear_health,
        false,
    )
}
pub(super) fn prepare_stats(
    source: &bace_content::WeenieV1,
    character: &bace_character::CharacterProgression,
    registry: &bace_magic::EnchantmentRegistry,
    dat: &PreparedAvatarDat,
    gear_health: u32,
    clamp: bool,
) -> Result<PlayerStatProjection, String> {
    let mut base_attributes = [0; 6];
    let mut current_attributes = [0; 6];
    let attribute_modifiers = [PreparedAttributeModifier::default(); 6];
    for index in 0..6 {
        let p = character
            .projection(ProgressionTarget::Attribute(
                AttributeId::try_from(index as u32 + 1).map_err(|_| "attribute id")?,
            ))
            .ok_or("missing player attribute")?;
        let Some(TraitDetails::Attribute { starting_value }) = p.details else {
            return Err("missing player attribute metadata".into());
        };
        let base = starting_value
            .checked_add(u32::from(p.ranks))
            .ok_or("player attribute overflow")?;
        let (multiplier, additive) = modifiers(registry, 1, index as u32 + 1)?;
        base_attributes[index] = base;
        current_attributes[index] =
            bace_character::project_attribute_value(base, multiplier, additive)
                .map_err(|e| format!("player attribute: {e:?}"))?;
    }
    let positive = |id| {
        source
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(Ok(0), |p| {
                u32::try_from(p.value).map_err(|_| "negative player augmentation")
            })
    };
    let vitae = registry
        .entries()
        .iter()
        .find(|e| e.spell == 666)
        .map_or(1., |e| e.spec.value.min(1.));
    let bonuses = SkillBonuses {
        all_skills: positive(365)?,
        skilled_melee: positive(300)?,
        skilled_missile: positive(301)?,
        skilled_magic: positive(302)?,
        enlightenment: positive(390)?,
        jack_of_all_trades: positive(326)?,
        specialized_luminance: positive(344)?,
    };
    let mut inputs = Vec::new();
    let mut physical_skills = Vec::new();
    for row in &source.properties.skills {
        let id = u32::try_from(row.id).map_err(|_| "skill id")?;
        let skill = dat
            .character
            .skill_table()
            .skills
            .get(&id)
            .ok_or("missing player DAT skill")?;
        let (multiplier, additive) = modifiers(registry, 0x10, id)?;
        let input = SkillValueInputs {
            formula: formula(skill.formula),
            usable_untrained: skill.min_level == 1,
            base_attributes,
            current_attributes,
            bonuses,
            multiplier,
            vitae,
            additive,
        };
        let value = character
            .skill_values(id, input)
            .map_err(|e| format!("player skill: {e:?}"))?;
        inputs.push((
            id,
            SkillValueInputs {
                multiplier: 1.0,
                additive: 0,
                vitae: 1.0,
                ..input
            },
        ));
        physical_skills.push((
            id,
            PhysicalSkill {
                advancement: value.advancement as u32,
                current: value.current,
            },
        ));
    }
    inputs.sort_by_key(|(id, _)| *id);
    physical_skills.sort_by_key(|(id, _)| *id);
    let mut vitals = [bace_entity::VitalPool {
        current: 0,
        maximum: 0,
    }; 3];
    let mut base_vitals = [0; 3];
    for (index, (id, f)) in [1, 3, 5]
        .into_iter()
        .zip([dat.vitals.health, dat.vitals.stamina, dat.vitals.mana])
        .enumerate()
    {
        let p = character
            .projection(ProgressionTarget::Vital(
                VitalId::try_from(id).map_err(|_| "vital id")?,
            ))
            .ok_or("missing player vital")?;
        let Some(TraitDetails::Vital { starting_value, .. }) = p.details else {
            return Err("missing vital metadata".into());
        };
        let multiplier = bace_magic::enchantment_modifiers(registry, 2 | 0x4000, id)
            .iter()
            .fold(1f32, |v, e| v * e.spec.value);
        let additive = bace_magic::enchantment_modifiers(registry, 2 | 0x8000, id)
            .iter()
            .fold(0f32, |v, e| v + e.spec.value);
        let base_bonus = if index == 0 {
            bonuses
                .enlightenment
                .checked_mul(2)
                .and_then(|v| v.checked_add(gear_health))
                .ok_or("health bonus overflow")?
        } else {
            0
        };
        base_vitals[index] =
            bace_character::project_vital_values(bace_character::VitalValueInputs {
                formula: formula(f),
                starting_value,
                ranks: u32::from(p.ranks),
                current_attributes: base_attributes,
                base_bonus,
                multiplier: 1.,
                vitae: 1.,
                additive: 0.,
            })
            .map_err(|e| format!("player base vital: {e:?}"))?
            .before_multipliers;
        let maximum = bace_character::project_vital_values(bace_character::VitalValueInputs {
            formula: formula(f),
            starting_value,
            ranks: u32::from(p.ranks),
            current_attributes,
            base_bonus,
            multiplier,
            vitae,
            additive,
        })
        .map_err(|e| format!("player vital: {e:?}"))?
        .maximum;
        let current = source
            .properties
            .secondary_attributes
            .iter()
            .find(|p| p.id == id)
            .ok_or("saved vital missing")?
            .value
            .current_level;
        if current > maximum && !clamp {
            return Err("saved vital exceeds prepared maximum; reconciliation required".into());
        }
        vitals[index] = bace_entity::VitalPool {
            current: current.min(maximum),
            maximum,
        };
    }
    Ok(PlayerStatProjection {
        base_attributes,
        current_attributes,
        vitals,
        base_vitals,
        physical_skills,
        skills: PreparedCharacterSkillInputs {
            attack_skill: 44,
            inputs,
            shield: None,
            attribute_modifiers,
        },
    })
}
