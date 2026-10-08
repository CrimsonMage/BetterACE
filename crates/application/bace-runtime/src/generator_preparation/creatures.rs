//! Native unarmed creature projection from content and verified DAT tables.
//! ACE CreatureVital/CreatureSkill/AttributeFormula and authored CMT/BodyPart
//! provide values; operational corpse/think policy is explicit host input.
use bace_content::WeenieTemplate;
use bace_dat::{Animation, CombatManeuverTable, MotionTable, SkillFormula, SkillTable, VitalTable};
use bace_gameplay_api::weapon_combat::*;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Copy, Debug)]
pub struct CreatureAdmissionPolicy {
    /// Resolve the source class name `corpse` from the accepted content catalog.
    pub corpse_template: u32,
    pub think_interval: u64,
    pub corpse_decay_ticks: u64,
}
pub struct CreatureAdmissionAssets<'a> {
    pub motions: &'a MotionTable,
    pub maneuvers: Option<&'a CombatManeuverTable>,
    pub animations: &'a BTreeMap<u32, Animation>,
    pub skills: &'a SkillTable,
    pub quality_filter: &'a bace_dat::QualityFilter,
    pub vitals: &'a VitalTable,
    pub physical: &'a crate::region_activation::PreparedPhysicalTemplate,
}
pub fn prepare_creature(
    template: &WeenieTemplate,
    assets: CreatureAdmissionAssets<'_>,
    policy: CreatureAdmissionPolicy,
) -> Result<bace_simulation::GeneratedNpcTemplate, String> {
    if template.weenie_type == 61 {
        return Err("GamePiece creature actor admission requires the chess owner".into());
    }
    let props = &template.properties;
    if !crate::generator_preparation::is_creature_template(template.weenie_type)
        || policy.corpse_template == 0
        || policy.think_interval == 0
        || policy.corpse_decay_ticks == 0
    {
        return Err("invalid native creature/policy".into());
    }
    let requires_equipment = props
        .create_list
        .iter()
        .any(|e| e.destination_type & 2 != 0)
        || props
            .data_ids
            .iter()
            .any(|p| matches!(p.id, 32 | 33) && p.value != 0);
    let attr = |id| -> Result<u32, String> {
        let a = props
            .attributes
            .iter()
            .find(|p| p.id == id)
            .ok_or("missing creature attribute")?;
        a.value
            .init_level
            .checked_add(a.value.level_from_cp)
            .ok_or_else(|| "attribute overflow".into())
    };
    let formula = |f: SkillFormula| -> Result<u32, String> {
        if f.x == 0 {
            return Ok(0);
        }
        if f.z == 0 {
            return Err("zero attribute formula divisor".into());
        }
        let mut total = attr(f.attribute1)?;
        if f.attribute2 != 0 {
            total = total
                .checked_add(attr(f.attribute2)?)
                .ok_or("formula overflow")?;
        }
        Ok(if f.z == 1 {
            total
        } else {
            (total as f32 / f.z as f32).round() as u32
        })
    };
    let max_health = props
        .secondary_attributes
        .iter()
        .find(|p| p.id == 1)
        .map(|p| p.value.init_level.checked_add(p.value.level_from_cp))
        .unwrap_or(Some(0))
        .and_then(|base| base.checked_add(formula(assets.vitals.health).ok()?))
        .filter(|v| *v > 0)
        .ok_or("invalid creature maximum health")?;
    let resource = |id, formula_value: SkillFormula| -> Result<bace_entity::VitalPool, String> {
        let initial = props
            .secondary_attributes
            .iter()
            .find(|p| p.id == id)
            .map_or(Some(0), |p| {
                p.value.init_level.checked_add(p.value.level_from_cp)
            })
            .ok_or("creature vital overflow")?;
        let maximum = initial
            .checked_add(formula(formula_value)?)
            .filter(|v| *v <= i32::MAX as u32)
            .ok_or("creature vital overflow")?;
        // Creature constructor resets all non-player vitals to their maximum.
        Ok(bace_entity::VitalPool {
            current: maximum,
            maximum,
        })
    };
    let resources = [
        resource(3, assets.vitals.stamina)?,
        resource(5, assets.vitals.mana)?,
    ];
    let mut skills = Vec::new();
    for row in &props.skills {
        let id = u32::try_from(row.id).map_err(|_| "invalid creature skill")?;
        let dat = assets.skills.skills.get(&id);
        let usable =
            row.value.sac >= 2 || (row.value.sac == 1 && dat.is_some_and(|s| s.min_level == 1));
        let base = if usable {
            dat.map(|s| formula(s.formula)).transpose()?.unwrap_or(0)
        } else {
            0
        };
        let current = base
            .checked_add(row.value.init_level)
            .and_then(|v| v.checked_add(u32::from(row.value.level_from_pp)))
            .ok_or("skill overflow")?;
        skills.push((
            id,
            PhysicalSkill {
                advancement: row.value.sac,
                current,
            },
        ));
    }
    skills.sort_by_key(|(id, _)| *id);
    let quality = |raw| PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    };
    let mut armor = Vec::new();
    let mut attacks = Vec::new();
    for row in &props.body_parts {
        let part = u32::try_from(row.id).map_err(|_| "negative creature body part")?;
        let b = &row.value;
        attacks.push(PhysicalBodyAttack {
            part,
            damage: f64::from(b.d_val),
            variance: b.d_var,
            damage_type: u32::try_from(b.d_type).map_err(|_| "invalid body damage type")?,
        });
        armor.push(PhysicalBodyDefense {
            part,
            hit_weights: [
                b.llf, b.llb, b.lrf, b.lrb, b.mlf, b.mlb, b.mrf, b.mrb, b.hlf, b.hlb, b.hrf, b.hrb,
            ],
            armor: [
                b.armor_vs_slash,
                b.armor_vs_pierce,
                b.armor_vs_bludgeon,
                b.armor_vs_cold,
                b.armor_vs_fire,
                b.armor_vs_acid,
                b.armor_vs_electric,
                b.armor_vs_nether,
            ]
            .map(|v| quality(f64::from(v))),
        });
    }
    armor.sort_by_key(|b| b.part);
    attacks.sort_by_key(|b| b.part);
    let maneuvers = match assets.maneuvers {
        Some(table) => crate::world_admission::prepare_combat_maneuvers(
            table,
            assets.motions,
            assets.animations,
        )?,
        None => crate::physical_assets::prepare_fallback_melee_motions(
            assets.motions,
            assets.animations,
            0x8000003c,
        )?,
    };
    let mut missile_timings = Vec::new();
    let mut missile_timing_errors = Vec::new();
    for style in [0x8000003f, 0x80000041, 0x80000043, 0x80000047, 0x8000013b] {
        let Some(&ready) = assets.motions.style_defaults.get(&style) else {
            continue;
        };
        let Ok(aim) =
            crate::world_admission::transition_data(assets.motions, style, ready, 0x4000001e)
        else {
            continue;
        };
        let timing = (|| -> Result<bace_simulation::PreparedNpcMissileTiming, String> {
            let launch_seconds =
                crate::world_admission::prepare_motion(aim, assets.animations)?.duration_seconds();
            let reload = crate::world_admission::transition_data(
                assets.motions,
                style,
                0x4000001e,
                0x40000016,
            )
            .ok();
            let reload_seconds = reload
                .map(|m| crate::world_admission::prepare_motion(m, assets.animations))
                .transpose()?
                .map_or(0.0, |m| m.duration_seconds());
            let from = if reload_seconds > 0.0 {
                0x40000016
            } else {
                0x4000001e
            };
            let back = crate::world_admission::transition_data(assets.motions, style, from, ready)?;
            let return_seconds =
                crate::world_admission::prepare_motion(back, assets.animations)?.duration_seconds();
            Ok(bace_simulation::PreparedNpcMissileTiming {
                style,
                launch_seconds,
                reload_seconds,
                return_seconds,
            })
        })();
        match timing {
            Ok(timing) => missile_timings.push(timing),
            Err(error) => missile_timing_errors.push((style, error)),
        };
    }
    let unarmed: Vec<_> = maneuvers
        .iter()
        .filter(|m| requires_equipment || m.style == 0x8000003c)
        .collect();
    let first = unarmed.first().copied();
    if assets.maneuvers.is_some() && first.is_none() {
        return Err("creature lacks unarmed CMT maneuvers".into());
    }
    let current = *assets
        .motions
        .style_defaults
        .get(&0x8000003d)
        .ok_or("missing creature noncombat motion")?;
    let death =
        crate::world_admission::transition_data(assets.motions, 0x8000003d, current, 0x40000011)?;
    let death =
        crate::world_admission::prepare_motion(death, assets.animations)?.duration_seconds();
    let float = |id, default| {
        props
            .floats
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let int = |id, default| {
        props
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let boolean = |id, default| {
        props
            .bools
            .iter()
            .find(|p| p.id == id)
            .map_or(default, |p| p.value)
    };
    let locomotion = assets
        .physical
        .locomotion
        .as_ref()
        .map_err(Clone::clone)?
        .clone();
    let run = skills
        .iter()
        .find(|(id, _)| *id == 24)
        .map_or(0, |(_, s)| s.current);
    let run_rate = bace_character::run_rate(bace_character::RunInput {
        skill: run,
        burden: 0.0,
        scale: 1.0,
        exhausted: false,
    })
    .map_err(|e| format!("invalid creature run: {e:?}"))?;
    let maximum_speed = (locomotion.profile.run.velocity * run_rate)
        .length_squared()
        .sqrt();
    let mut range = 0.0f32;
    for maneuver in &unarmed {
        let ready = *assets
            .motions
            .style_defaults
            .get(&maneuver.style)
            .ok_or("missing stance default")?;
        let data = crate::world_admission::transition_data(
            assets.motions,
            maneuver.style,
            ready,
            maneuver.motion,
        )?;
        let motion = crate::world_admission::prepare_motion(data, assets.animations)?;
        for hook in motion.hooks() {
            if let bace_motion::MotionHookPayload::Attack { radius, .. } = hook.payload {
                range = range.max(radius);
            }
        }
    }
    if !range.is_finite() || (!unarmed.is_empty() && range <= 0.0) {
        return Err("missing positive authored attack radius".into());
    }

    let content_hash = sha2::Sha256::digest(
        bace_content_tools::compile_template(template).map_err(|e| e.to_string())?,
    )
    .into();
    let profile = PhysicalCombatProfile {
        equipment: vec![],
        revision: 0,
        content_hash,
        player: false,
        creature_type: u32::try_from(int(2, 0)).map_err(|_| "invalid creature type")?,
        pk: PkStatus::Npk,
        attackable: boolean(19, true),
        immune: false,
        lifestone_protected: false,
        style: if maneuvers.is_empty() {
            assets.motions.default_style
        } else {
            0x8000003c
        },
        main: None,
        offhand: None,
        launcher: None,
        ammunition: None,
        gloves: None,
        boots: None,
        body_attacks: attacks.clone(),
        maneuvers: maneuvers.clone(),
        skills,
        strength: attr(1)?,
        coordination: attr(4)?,
        quickness: attr(3)?,
        base_strength: attr(1)?,
        base_endurance: attr(2)?,
        melee_defense_modifier: 1.0,
        missile_defense_modifier: 1.0,
        armor,
        resistances: [64, 65, 66, 68, 67, 69, 70, 166].map(|id| PhysicalResistance {
            quality: quality(float(id, 1.0)),
            augmentation: 0,
        }),
        shield_encumbrance: 0,
        shield_placement: false,
        armor_layers: vec![],
        ignore_shield: 0.0,
        shield: None,
        shield_skill: PhysicalSkill {
            advancement: 0,
            current: 0,
        },
        ratings: PhysicalRatings::default(),
        critical_defense: false,
        range,
        height: assets.physical.shape.height(),
        missile: None,
    };
    let maximum_turn_rate = locomotion.profile.turn.omega.z.abs();
    let geometry = bace_simulation::PreparedNpcGeometry {
        shape: assets.physical.shape.clone(),
        locomotion,
        heading: 0.0,
        maximum_turn_rate,
        run_rate,
    };
    let blueprint = bace_simulation::NpcBlueprint {
        cell: bace_types::CellId(0),
        position: bace_geometry::Vec3::ZERO,
        radius: geometry.shape.horizontal_radius(),
        capabilities: bace_motion::Capabilities {
            speed: maximum_speed,
            jump_impulse: 0.0,
        },
        combat: bace_entity::CombatantProfile {
            maximum_health: max_health,
            melee_damage: first.map_or(0, |_| {
                attacks.iter().map(|a| a.damage as u32).max().unwrap_or(0)
            }),
            melee_range: range,
            attack_duration: first.map_or(0.0, |first| first.duration),
            strike_offsets: first.map_or_else(Vec::new, |first| {
                first.hooks.iter().map(|h| h.seconds).collect()
            }),
            player: false,
        },
        visual_range: float(31, 18.0) as f32,
        think_interval: policy.think_interval,
        corpse_template: policy.corpse_template,
        xp_override: props.ints.iter().find(|p| p.id == 146).map(|p| p.value),
        loot: vec![],
        death_animation_ticks: (death * 30.0).ceil() as u64,
        respawn_ticks: 0,
        corpse_decay_ticks: policy.corpse_decay_ticks,
    };
    let profile = Arc::new(profile);
    let combat_assets = Arc::new(crate::npc_combat_assets::prepare_npc_combat_assets(
        Arc::new(template.clone()),
        profile.clone(),
        &[],
        assets.skills,
        assets.quality_filter,
    )?);
    let (physical_motions, locomotion_styles) = crate::physical_assets::prepare_npc_motion_assets(
        &profile,
        crate::physical_assets::PhysicalMotionAssets {
            table: assets.motions,
            animations: assets.animations,
            current_motion: 0x41000003,
            current_speed: 1.0,
            scale: float(39, 1.0) as f32,
            modifiers: &[],
        },
    )?;
    Ok(bace_simulation::GeneratedNpcTemplate {
        script: None,
        combat_assets: Some(combat_assets),
        death_motions: crate::physical_assets::prepare_death_motion_assets(
            assets.motions,
            assets.animations,
            &locomotion_styles,
            float(39, 1.0) as f32,
        )?,
        physical_motions,
        locomotion_styles,
        requires_equipment,
        resources,
        missile_timings,
        missile_timing_errors,
        blueprint,
        geometry,
        physical: Some(profile),
        loot: None,
        ace_loot: None,
    })
}
use sha2::Digest;
