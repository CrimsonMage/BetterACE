//! Cold DAT preparation for physical actions and reload. No tick performs asset
//! lookups and missing source links never fall back to synthetic hook deadlines.
use bace_dat::{Animation, MotionTable};
use bace_gameplay_api::weapon_combat::PhysicalCombatProfile;
use bace_motion::PreparedMotionChain;
use std::{collections::BTreeMap, sync::Arc};

pub type PhysicalMotionRegistration = (u32, f32, Arc<PreparedMotionChain>);
pub struct PhysicalMotionAssets<'a> {
    pub table: &'a MotionTable,
    pub animations: &'a BTreeMap<u32, Animation>,
    pub current_motion: u32,
    pub current_speed: f32,
    pub scale: f32,
    pub modifiers: &'a [(u32, f32)],
}
pub fn prepare_physical_motion_assets(
    profile: &PhysicalCombatProfile,
    assets: PhysicalMotionAssets<'_>,
) -> Result<Vec<PhysicalMotionRegistration>, String> {
    prepare_physical_motion_assets_with_previous_style(profile, assets, None)
}
/// Equipment can change the physical style while the accepted body still moves
/// in its prior style. Retain that exact source transition in the cold closure.
pub fn prepare_physical_motion_assets_with_previous_style(
    profile: &PhysicalCombatProfile,
    assets: PhysicalMotionAssets<'_>,
    previous_style: Option<u32>,
) -> Result<Vec<PhysicalMotionRegistration>, String> {
    bace_combat::physical::validate_physical_profile(profile)
        .map_err(|e| format!("invalid physical profile: {e:?}"))?;
    let mut requests: Vec<(u32, f32)> = Vec::new();
    let mut add = |motion, speed: f32| -> Result<(), String> {
        if requests.iter().any(|&(m, s)| m == motion && s == speed) {
            return Ok(());
        }
        if requests.len() == 128 {
            return Err("physical motion variant capacity".into());
        }
        requests.push((motion, speed));
        Ok(())
    };
    let speed = bace_combat::physical::attack_speed(
        profile.quickness,
        profile.main.as_ref().map_or(0, |w| w.attack_time),
    )
    .map_err(|e| format!("physical speed: {e:?}"))?;
    for m in profile
        .maneuvers
        .iter()
        .filter(|m| m.style == profile.style)
    {
        add(m.motion, speed)?;
    }
    if let Some(offhand) = &profile.offhand {
        let speed = bace_combat::physical::attack_speed(profile.quickness, offhand.attack_time)
            .map_err(|e| format!("offhand speed: {e:?}"))?;
        for m in profile
            .maneuvers
            .iter()
            .filter(|m| m.style == profile.style)
        {
            add(m.motion, speed)?;
        }
    }
    if let Some(missile) = profile.missile {
        let launcher = profile
            .launcher
            .as_ref()
            .ok_or("missing missile launcher")?;
        let speed =
            bace_combat::physical::missile_attack_speed(profile.quickness, launcher.attack_time)
                .map_err(|e| format!("missile speed: {e:?}"))?;
        add(missile.attack_motion, speed)?;
        add(0x40000016, speed)?;
    }
    let mut result: Vec<PhysicalMotionRegistration> = requests
        .into_iter()
        .map(|(motion, speed)| {
            let chain = crate::world_admission::prepare_motion_chain(
                assets.table,
                assets.animations,
                crate::world_admission::MotionChainRequest {
                    style: profile.style,
                    current_motion: assets.current_motion,
                    current_speed: assets.current_speed,
                    action: motion,
                    action_speed: speed,
                    scale: assets.scale,
                    modifiers: assets.modifiers,
                },
            )?;
            if chain.stop_chain().is_none() || chain.nominal_duration_seconds() > 150.0 {
                return Err("physical chain missing bounded Ready suffix".into());
            }
            Ok((motion, speed, chain))
        })
        .collect::<Result<_, String>>()?;
    let mut targets = vec![profile.style];
    if profile.player || assets.table.style_defaults.contains_key(&0x80000049) {
        targets.push(0x80000049);
    }
    if profile.player || assets.table.style_defaults.contains_key(&0x8000003d) {
        targets.push(0x8000003d);
    }
    targets.sort_unstable();
    targets.dedup();
    for target in targets.iter().copied() {
        for (&style, &substate) in &assets.table.style_defaults {
            if style == target
                || !assets
                    .table
                    .cycles
                    .contains_key(&(target.wrapping_shl(16) | (substate & 0xffffff)))
            {
                continue;
            }
            let chain = crate::world_admission::prepare_motion_chain(
                assets.table,
                assets.animations,
                crate::world_admission::MotionChainRequest {
                    style,
                    current_motion: substate,
                    current_speed: 1.0,
                    action: target,
                    action_speed: 1.0,
                    scale: assets.scale,
                    modifiers: &[],
                },
            )?;
            if result.len() == 128 {
                return Err("physical action/style chain capacity".into());
            }
            result.push((target, 1.0, chain));
        }
    }
    let mut live_styles = vec![assets.table.default_style, profile.style];
    if let Some(style) = previous_style {
        if !assets.table.style_defaults.contains_key(&style) {
            return Err("prior equipment motion style absent".into());
        }
        live_styles.push(style);
    }
    if targets.contains(&0x80000049) {
        live_styles.push(0x80000049);
    };
    live_styles.sort_unstable();
    live_styles.dedup();
    for style in live_styles {
        if !assets.table.style_defaults.contains_key(&style) {
            continue;
        }
        for target in targets.iter().copied() {
            let action = if style == target { 0x41000003 } else { target };
            for (current, speed) in [(0x45000005, 1.0), (0x45000005, -1.0), (0x44000007, 1.0)] {
                let chain = crate::world_admission::prepare_motion_chain(
                    assets.table,
                    assets.animations,
                    crate::world_admission::MotionChainRequest {
                        style,
                        current_motion: current,
                        current_speed: speed,
                        action,
                        action_speed: 1.0,
                        scale: assets.scale,
                        modifiers: &[],
                    },
                )?;
                if result.len() == 128 {
                    return Err("physical live locomotion variant capacity".into());
                }
                result.push((action, 1.0, chain));
            }
        }
    }
    Ok(result)
}
/// The complete legal client style closure; arbitrary packet styles never select
/// another DAT table or capability set.
pub fn prepare_locomotion_styles(
    table: &MotionTable,
    animations: &BTreeMap<u32, Animation>,
    physical_style: u32,
    scale: f32,
) -> Result<Vec<Arc<bace_motion::AnimatedLocomotion>>, String> {
    let mut styles = vec![0x8000003d, 0x80000049, physical_style];
    styles.sort_unstable();
    styles.dedup();
    styles
        .into_iter()
        .map(|style| {
            crate::world_admission::prepare_animated_locomotion(table, style, animations, scale)
        })
        .collect()
}

pub fn prepare_npc_motion_assets(
    profile: &PhysicalCombatProfile,
    assets: PhysicalMotionAssets<'_>,
) -> Result<
    (
        Vec<PhysicalMotionRegistration>,
        Vec<Arc<bace_motion::AnimatedLocomotion>>,
    ),
    String,
> {
    let mut styles = vec![assets.table.default_style, profile.style];
    if assets.table.style_defaults.contains_key(&0x80000049) {
        styles.push(0x80000049);
    }
    styles.sort_unstable();
    styles.dedup();
    let locomotion = styles
        .into_iter()
        .map(|style| {
            crate::world_admission::prepare_animated_locomotion(
                assets.table,
                style,
                assets.animations,
                assets.scale,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((prepare_physical_motion_assets(profile, assets)?, locomotion))
}

pub fn prepare_death_motion_assets(
    table: &MotionTable,
    animations: &BTreeMap<u32, Animation>,
    styles: &[Arc<bace_motion::AnimatedLocomotion>],
    scale: f32,
) -> Result<Vec<bace_motion::PreparedDeathMotion>, String> {
    if styles.is_empty() || styles.len() > 4 {
        return Err("death style closure bounds".into());
    }
    let mut result = Vec::with_capacity(styles.len() * 4);
    for style in styles {
        let style = style.profile.style;
        let dead = crate::world_admission::prepare_motion_chain(
            table,
            animations,
            crate::world_admission::MotionChainRequest {
                style,
                current_motion: 0x41000003,
                current_speed: 1.0,
                action: 0x40000011,
                action_speed: 1.0,
                scale,
                modifiers: &[],
            },
        )?;
        for (current, speed) in [
            (0x41000003, 1.0),
            (0x45000005, 1.0),
            (0x45000005, -1.0),
            (0x44000007, 1.0),
        ] {
            let stop = crate::world_admission::prepare_motion_chain(
                table,
                animations,
                crate::world_admission::MotionChainRequest {
                    style,
                    current_motion: current,
                    current_speed: speed,
                    action: 0x41000003,
                    action_speed: 1.0,
                    scale,
                    modifiers: &[],
                },
            )?;
            result.push(bace_motion::PreparedDeathMotion {
                stop,
                dead: dead.clone(),
            });
        }
    }
    Ok(result)
}

/// Missing CMT is legal in GDLE. Only real DAT fallback actions carrying attack
/// hooks become melee capabilities; absent links are not replaced with timers.
pub fn prepare_fallback_melee_motions(
    motions: &MotionTable,
    animations: &BTreeMap<u32, Animation>,
    style: u32,
) -> Result<Vec<bace_gameplay_api::weapon_combat::PhysicalManeuver>, String> {
    use bace_gameplay_api::weapon_combat::{PhysicalAttackHook, PhysicalManeuver};
    let Some(&ready) = motions.style_defaults.get(&style) else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for height in 1..=3 {
        for power in [0.0, 0.5, 1.0] {
            let motion = bace_combat::physical::fallback_melee_motion(height, power, false)
                .map_err(|e| format!("fallback attack: {e:?}"))?;
            let Ok(data) = crate::world_admission::transition_data(motions, style, ready, motion)
            else {
                continue;
            };
            let timeline = crate::world_admission::prepare_motion(data, animations)?;
            let hooks: Vec<_> = timeline
                .hooks()
                .iter()
                .filter_map(|hook| match hook.payload {
                    bace_motion::MotionHookPayload::Attack { part, .. } => {
                        Some(PhysicalAttackHook {
                            seconds: hook.seconds,
                            part,
                        })
                    }
                    _ => None,
                })
                .collect();
            if hooks.is_empty() {
                continue;
            }
            result.push(PhysicalManeuver {
                style,
                attack_type: 0,
                height,
                minimum_skill: 0,
                motion,
                duration: timeline.duration_seconds(),
                hooks,
            });
        }
    }
    Ok(result)
}
