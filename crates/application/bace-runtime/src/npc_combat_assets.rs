//! Pure NPC combat join from actual native sources and DAT rules. No mutable
//! player progression, account, guessed entity identity or asset I/O is created.
use bace_combat::preparation::{
    PhysicalEquipmentSource, PhysicalQualityFamily as F, PhysicalQualityProjection,
};
use bace_gameplay_api::{
    SkillAdvancement,
    weapon_combat::{PhysicalCombatProfile, PhysicalQuality},
};
use bace_simulation::{MagicCaster, PreparedNpcCombatAssets, PreparedNpcQuality, PreparedNpcSkill};
use std::{collections::BTreeSet, sync::Arc};
pub fn prepare_npc_combat_assets(
    source: Arc<bace_content::WeenieV1>,
    physical: Arc<PhysicalCombatProfile>,
    equipment: &[PhysicalEquipmentSource],
    skills: &bace_dat::SkillTable,
    filter: &bace_dat::QualityFilter,
) -> Result<PreparedNpcCombatAssets, String> {
    source
        .validate(Default::default())
        .map_err(|e| e.to_string())?;
    bace_combat::physical::validate_physical_profile(&physical)
        .map_err(|e| format!("NPC physical profile: {e:?}"))?;
    if physical.player
        || equipment.len() > 128
        || source.properties.skills.len() > 256
        || equipment.len() != physical.equipment.len()
    {
        return Err("NPC combat source bounds/role".into());
    }
    let mut equipment_seen = BTreeSet::new();
    for item in equipment {
        if item.entity == 0
            || !equipment_seen.insert(item.entity)
            || physical.equipment.iter().all(|p| {
                p.entity != item.entity
                    || p.revision != item.revision
                    || p.location != item.location
            })
        {
            return Err("NPC equipment source identity".into());
        }
        item.weenie
            .validate(Default::default())
            .map_err(|e| e.to_string())?;
    }
    let mut base_attributes = [0; 6];
    for (index, value) in base_attributes.iter_mut().enumerate() {
        let row = source
            .properties
            .attributes
            .iter()
            .find(|p| p.id == index as u32 + 1)
            .ok_or("NPC base attribute missing")?;
        *value = row
            .value
            .init_level
            .checked_add(row.value.level_from_cp)
            .ok_or("NPC attribute overflow")?;
    }
    let mut inputs = Vec::new();
    let mut skill_sources = Vec::new();
    let mut values = Vec::new();
    for row in &source.properties.skills {
        let skill = u32::try_from(row.id).map_err(|_| "NPC skill ID")?;
        let dat = skills.skills.get(&skill);
        let advancement =
            SkillAdvancement::try_from(row.value.sac).map_err(|_| "NPC skill advancement")?;
        let formula = dat.map(|s| s.formula);
        let input = bace_character::SkillValueInputs {
            formula: bace_character::VitalFormula {
                enabled: formula.is_some_and(|f| f.x != 0),
                divisor: formula.map_or(1, |f| f.z),
                attribute1: formula.map_or(1, |f| f.attribute1),
                attribute2: formula.map_or(0, |f| f.attribute2),
            },
            usable_untrained: dat.is_some_and(|s| s.min_level == 1),
            base_attributes,
            current_attributes: base_attributes,
            bonuses: Default::default(),
            multiplier: 1.,
            vitae: 1.,
            additive: 0,
        };
        let value = bace_character::project_skill_values(
            skill,
            advancement,
            row.value.level_from_pp,
            row.value.init_level,
            input,
        )
        .map_err(|e| format!("NPC skill value: {e:?}"))?;
        if physical
            .skills
            .iter()
            .find(|(id, _)| *id == skill)
            .is_none_or(|(_, p)| p.current != value.current || p.advancement != row.value.sac)
        {
            return Err("NPC physical/DAT skill join mismatch".into());
        }
        inputs.push((skill, input));
        skill_sources.push(PreparedNpcSkill {
            skill,
            advancement,
            ranks: row.value.level_from_pp,
            initial_level: row.value.init_level,
        });
        values.push(value);
    }
    let value = |id| {
        values
            .iter()
            .find(|v| v.skill == id)
            .map_or(0, |v| v.current)
    };
    let known_spells = source
        .properties
        .spell_book
        .iter()
        .map(|p| u32::try_from(p.id).map_err(|_| "NPC spell ID"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if known_spells.len() > 8192 || known_spells.iter().any(|id| *id == 0 || *id > 65535) {
        return Err("NPC spellbook bounds".into());
    }
    let caster = MagicCaster {
        player: false,
        known_spells,
        school_skills: [34, 33, 31, 32, 43].map(value),
        magic_defense: value(15),
        mana_conversion: value(16),
        components_required: false,
        safe_components: true,
    };
    let equipment_magic = equipment
        .iter()
        .map(|e| {
            Ok(bace_magic::MagicDamageEquipment {
                entity: e.entity,
                revision: e.revision,
                location: e.location,
                level: crate::item_experience::read_experience(&e.weenie, e.revision)?
                    .map(|xp| xp.level().map_err(|err| format!("NPC item level: {err:?}")))
                    .transpose()?,
                weenie: &e.weenie,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let damage = bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
        player: false,
        weenie: &source,
        equipment: &equipment_magic,
        skills: &physical.skills,
        base_attributes,
        base_shield_skill: values.iter().find(|s| s.skill == 48).map_or(0, |s| s.base),
    })
    .map_err(|e| format!("NPC magic profile: {e:?}"))?;
    let actor_qualities = raw_qualities(&source, filter)?;
    let mut equipment_qualities = Vec::new();
    for item in equipment {
        equipment_qualities.extend(raw_qualities(&item.weenie, filter)?.into_iter().map(|q| {
            PhysicalQualityProjection {
                entity: item.entity,
                family: q.family,
                stat: q.stat,
                part: q.part,
                details: q.details,
                value: q.value,
            }
        }));
    }
    if actor_qualities.len() + equipment_qualities.len() > 32768 {
        return Err("NPC raw quality closure capacity".into());
    }
    let properties = prepare_npc_properties(&source)?;
    let attack_skill = physical
        .main
        .as_ref()
        .or(physical.launcher.as_ref())
        .map_or(45, |w| w.skill);
    let shield = physical.shield.as_ref().and_then(|s| {
        damage
            .shield
            .map(|(_, magic_absorption)| bace_simulation::PreparedShield {
                effective_armor: s.armor.raw as f32,
                magic_absorption,
            })
    });
    Ok(PreparedNpcCombatAssets {
        restored_registries: None,
        registry_items: equipment
            .iter()
            .map(|item| bace_types::EntityId(item.entity))
            .collect(),
        server_magic: bace_simulation::PreparedMagicAssetBatch {
            definitions: Vec::new(),
            projectile_shapes: Vec::new(),
        },
        caster,
        damage,
        source,
        properties,
        physical,
        equipment: equipment.to_vec(),
        actor_qualities,
        equipment_qualities,
        skill_inputs: bace_simulation::PreparedCharacterSkillInputs {
            attack_skill,
            inputs,
            shield,
            attribute_modifiers: [bace_simulation::PreparedAttributeModifier::default(); 6],
        },
        skills: skill_sources,
        base_attributes,
    })
}
fn raw_qualities(
    source: &bace_content::WeenieV1,
    filter: &bace_dat::QualityFilter,
) -> Result<Vec<PreparedNpcQuality>, String> {
    let mut result = Vec::new();
    let mut ints = BTreeSet::from([
        28, 44, 49, 56, 299, 307, 308, 309, 310, 314, 316, 329, 333, 334, 335, 336, 356, 357, 360,
        361, 370, 371, 374, 375, 381, 382, 383, 384,
    ]);
    ints.extend(source.properties.ints.iter().map(|p| p.id));
    let mut floats = BTreeSet::from([
        13, 14, 15, 16, 17, 18, 19, 62, 64, 65, 66, 67, 68, 69, 70, 136, 138, 147, 151, 165, 166,
        168,
    ]);
    floats.extend(source.properties.floats.iter().map(|p| p.id));
    if ints.len() + floats.len() > 2048 || source.properties.body_parts.len() > 256 {
        return Err("NPC source quality capacity".into());
    }
    let neutral = |raw| PhysicalQuality {
        raw,
        increasing: 1.,
        decreasing: 1.,
        additive_increasing: 0.,
        additive_decreasing: 0.,
    };
    for stat in ints.into_iter().filter(|id| filter.allows_int(*id)) {
        let raw = source
            .properties
            .ints
            .iter()
            .find(|p| p.id == stat)
            .map_or(0., |p| f64::from(p.value));
        result.push(PreparedNpcQuality {
            family: F::Int,
            stat,
            part: None,
            details: neutral(raw),
            value: raw,
        });
    }
    for stat in floats.into_iter().filter(|id| filter.allows_float(*id)) {
        let default = if [
            13, 14, 15, 16, 17, 18, 19, 64, 65, 66, 67, 68, 69, 70, 165, 166,
        ]
        .contains(&stat)
        {
            1.
        } else {
            0.
        };
        let raw = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == stat)
            .map_or(default, |p| p.value);
        result.push(PreparedNpcQuality {
            family: F::Float,
            stat,
            part: None,
            details: neutral(raw),
            value: raw,
        });
    }
    for p in &source.properties.body_parts {
        let b = &p.value;
        let part = u32::try_from(p.id).map_err(|_| "NPC body part")?;
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
            result.push(PreparedNpcQuality {
                family: F::BodyArmor,
                stat,
                part: Some(part),
                details: neutral(f64::from(raw)),
                value: f64::from(raw),
            });
        }
    }
    Ok(result)
}

/// All scalar families are initialized before the first physical hit. Script
/// admission reuses this same complete owner rather than creating a name-only bag.
pub fn prepare_npc_properties(
    source: &bace_content::WeenieV1,
) -> Result<bace_entity::EntityProperties, String> {
    use bace_entity::{PropertyFamily as F, PropertyValue as V};
    let p = &source.properties;
    if p.strings.iter().any(|p| p.id == 1 && p.value.len() > 1024) {
        return Err("NPC display name limit".into());
    }
    let values = p
        .bools
        .iter()
        .map(|p| (F::Bool, p.id, V::Bool(p.value)))
        .chain(p.ints.iter().map(|p| (F::Int, p.id, V::Int(p.value))))
        .chain(p.int64s.iter().map(|p| (F::Int64, p.id, V::Int64(p.value))))
        .chain(p.floats.iter().map(|p| (F::Float, p.id, V::Float(p.value))))
        .chain(
            p.strings
                .iter()
                .map(|p| (F::String, p.id, V::String(p.value.clone()))),
        )
        .collect();
    let properties = bace_entity::EntityProperties::restore_snapshot(0, values)
        .map_err(|e| format!("NPC source properties: {e:?}"))?;
    if properties
        .retained_bytes()
        .is_none_or(|n| n > 2 * 1024 * 1024)
    {
        return Err("NPC scalar quality byte capacity".into());
    }
    Ok(properties)
}
