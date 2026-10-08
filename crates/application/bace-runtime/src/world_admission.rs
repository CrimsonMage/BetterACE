//! Cold, immutable motion preparation. Asset reading remains with the bounded
//! preparation worker; these adapters perform no I/O and grant no world entry.
use bace_dat::{Animation, AnimationHookPayload, CombatManeuverTable, MotionData, MotionTable};
use bace_geometry::Vec3;
use bace_motion::{MotionHook, MotionHookPayload, MotionSegment, PreparedMotion};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct MotionChainRequest<'a> {
    pub style: u32,
    pub current_motion: u32,
    pub current_speed: f32,
    pub action: u32,
    pub action_speed: f32,
    pub scale: f32,
    pub modifiers: &'a [(u32, f32)],
}
/// GDLE CMotionTable.GetObjectSequence action branch. Resolves direct or
/// current→default→action→current links and retains the authored cyclic suffix.
/// Missing links, animations or unsupported root orientation fail preparation.
pub fn prepare_motion_chain(
    table: &MotionTable,
    animations: &BTreeMap<u32, Animation>,
    request: MotionChainRequest<'_>,
) -> Result<std::sync::Arc<bace_motion::PreparedMotionChain>, String> {
    prepare_motion_chain_inner(table, animations, request, true)
}
fn prepare_motion_chain_inner(
    table: &MotionTable,
    animations: &BTreeMap<u32, Animation>,
    request: MotionChainRequest<'_>,
    attach_stop: bool,
) -> Result<std::sync::Arc<bace_motion::PreparedMotionChain>, String> {
    use bace_motion::{ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame};
    let MotionChainRequest {
        style,
        current_motion,
        current_speed,
        action,
        action_speed,
        scale,
        modifiers,
    } = request;
    if style == 0
        || current_motion == 0
        || action & 0xd0000000 == 0
        || modifiers.len() > 16
        || [current_speed, action_speed, scale]
            .iter()
            .any(|v| !v.is_finite())
        || current_speed == 0.0
        || action_speed == 0.0
        || current_speed.abs() > 20.0
        || action_speed.abs() > 20.0
        || !(0.001..=100.0).contains(&scale)
    {
        return Err("invalid motion chain request".into());
    }
    let source = DatMotionSource(table);
    let resolved = bace_motion::resolve_sequence(
        &source,
        bace_motion::ActionChainRequest {
            style,
            current: current_motion,
            current_speed,
            action,
            action_speed,
        },
    )
    .map_err(|e| format!("motion chain lookup: {e:?} (style {style:#010x}, current {current_motion:#010x}, action {action:#010x})"))?;
    let cycle = resolved.cycle;
    let cycle_speed = resolved.final_speed;
    let cycle_rate = resolved.cycle_rate;
    let transition = bace_motion::SourceMotionTransition {
        before: bace_motion::SourceMotionState {
            style,
            substate: current_motion,
            speed: current_speed,
        },
        after: bace_motion::SourceMotionState {
            style: resolved.final_style,
            substate: resolved.final_substate,
            speed: resolved.final_speed,
        },
        continues_cycle: resolved.continues_cycle,
    };
    let completion_clips = resolved.completion_clips;
    if resolved.first_cyclic != completion_clips {
        return Err(
            "multi-clip cyclic warmup requires queued-tail execution; not yet qualified".into(),
        );
    }
    let modifiers = if action & 0x80000000 != 0 && cycle.bitfield & 1 != 0 {
        &[][..]
    } else {
        modifiers
    };
    if !modifiers.is_empty() && cycle.bitfield & 1 != 0 {
        return Err("current cycle forbids modifiers".into());
    }
    if modifiers
        .iter()
        .enumerate()
        .any(|(index, (id, _))| modifiers[..index].iter().any(|(old, _)| old == id))
    {
        return Err("duplicate current modifier".into());
    }
    let parts = resolved
        .parts
        .into_iter()
        .map(|part| (part.data, part.speed, part.rate));
    let mut clips = Vec::new();
    let mut clip_rates = Vec::new();
    for (data, speed, role) in parts {
        let start = if transition.continues_cycle {
            data.animations.len().saturating_sub(1)
        } else {
            0
        };
        for segment in &data.animations[start..] {
            let animation = animations
                .get(&segment.animation_id)
                .filter(|a| a.id == segment.animation_id)
                .ok_or_else(|| format!("missing chain animation {:08x}", segment.animation_id))?;
            if animation.frames.len() != animation.num_frames as usize {
                return Err("incomplete chain animation frames".into());
            }
            let frames = animation
                .position_frames
                .iter()
                .map(|frame| {
                    let q = frame.rotation;
                    let length = q.iter().map(|v| v * v).sum::<f32>();
                    if q[1].abs() > 0.0002
                        || q[2].abs() > 0.0002
                        || !(0.999..=1.001).contains(&length)
                    {
                        return Err(
                            "non-upright action root requires full orientation execution"
                                .to_owned(),
                        );
                    }
                    let heading = (2.0 * q[3].atan2(q[0]) + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    Ok(RootFrame {
                        translation: Vec3::new(frame.origin[0], frame.origin[1], frame.origin[2])
                            * scale,
                        heading,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            clips.push(ExecutionClip {
                animation: animation.id,
                frame_count: animation.num_frames,
                low: segment.low_frame,
                high: segment.high_frame,
                framerate: segment.framerate * speed,
                frames: frames.into(),
                hooks: animation_hooks(animation).into(),
            });
            clip_rates.push(bace_motion::MotionClipRate {
                base: segment.framerate,
                role,
            });
        }
    }
    let vector = |v: Option<[f32; 3]>| v.map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
    let mut physics = MotionPhysics {
        velocity: vector(cycle.velocity) * cycle_speed,
        omega: vector(cycle.omega) * cycle_speed,
    };
    let cycle_physics = MotionPhysics {
        velocity: vector(cycle.velocity),
        omega: vector(cycle.omega),
    };
    let mut modifier_physics = Vec::new();
    for (motion, speed) in modifiers {
        if !speed.is_finite() || speed.abs() > 20.0 {
            return Err("invalid motion modifier speed".into());
        }
        let data = table
            .modifiers
            .get(&(transition.after.style.wrapping_shl(16) | *motion))
            .or_else(|| table.modifiers.get(&(motion & 0xffffff)))
            .ok_or("missing current motion modifier")?;
        physics.velocity = physics.velocity + vector(data.velocity) * *speed;
        physics.omega = physics.omega + vector(data.omega) * *speed;
        modifier_physics.push((
            MotionPhysics {
                velocity: vector(data.velocity),
                omega: vector(data.omega),
            },
            *speed,
        ));
    }
    physics.velocity = physics.velocity * scale;
    let first_cyclic = clips.len().checked_sub(1).ok_or("empty cyclic suffix")?;
    let mut prepared = PreparedMotionChain::prepare(
        clips,
        first_cyclic,
        completion_clips,
        physics,
        action,
        action_speed,
    )
    .and_then(|chain| chain.with_source_transition(transition))
    .and_then(|chain| {
        chain.with_rate_model(bace_motion::MotionRateModel {
            clears_modifiers: cycle.bitfield & 1 != 0,
            clips: clip_rates,
            cycle_rate,
            cycle_physics,
            modifiers: modifier_physics,
            scale,
        })
    })
    .map_err(|e| format!("invalid prepared motion chain: {e:?}"))?;
    if attach_stop {
        let ready = table
            .style_defaults
            .get(&transition.after.style)
            .copied()
            .ok_or("missing stop default substate")?;
        let stop = prepare_motion_chain_inner(
            table,
            animations,
            MotionChainRequest {
                style: transition.after.style,
                current_motion: transition.after.substate,
                current_speed: transition.after.speed,
                action: ready,
                action_speed: 1.0,
                modifiers: &[],
                ..request
            },
            false,
        )?;
        prepared = prepared
            .with_stop_chain(stop)
            .map_err(|e| format!("invalid stop chain: {e:?}"))?;
    }
    Ok(std::sync::Arc::new(prepared))
}
struct DatMotionSource<'a>(&'a MotionTable);
impl bace_motion::MotionTableSource for DatMotionSource<'_> {
    type Data = MotionData;
    fn default_style(&self) -> u32 {
        self.0.default_style
    }
    fn bitfield(&self, data: &MotionData) -> u8 {
        data.bitfield
    }
    fn default_motion(&self, style: u32) -> Option<u32> {
        self.0.style_defaults.get(&style).copied()
    }
    fn link(&self, key: u32, motion: u32) -> Option<&MotionData> {
        self.0.links.get(&key)?.get(&motion)
    }
    fn cycle(&self, key: u32) -> Option<&MotionData> {
        self.0.cycles.get(&key)
    }
    fn animation_count(&self, data: &MotionData) -> usize {
        data.animations.len()
    }
}

pub fn prepare_animated_locomotion(
    table: &MotionTable,
    style: u32,
    animations: &BTreeMap<u32, Animation>,
    scale: f32,
) -> Result<std::sync::Arc<bace_motion::AnimatedLocomotion>, String> {
    use bace_motion::{AnimatedLocomotion, RootCycle, RootFrame, RootSegment};
    if !scale.is_finite() || !(0.001..=100.0).contains(&scale) {
        return Err("invalid locomotion scale".into());
    }
    let mut profile = prepare_locomotion(table, style)?;
    for physics in [
        &mut profile.ready,
        &mut profile.walk,
        &mut profile.run,
        &mut profile.sidestep,
        &mut profile.turn,
    ] {
        physics.velocity = physics.velocity * scale;
    }
    let cycle = |motion: u32| -> Result<RootCycle, String> {
        let data = table
            .cycles
            .get(&(style.wrapping_shl(16) | (motion & 0xffffff)))
            .or_else(|| {
                table
                    .cycles
                    .get(&(table.default_style.wrapping_shl(16) | (motion & 0xffffff)))
            })
            .ok_or("missing root cycle")?;
        let mut segments = Vec::new();
        for segment in &data.animations {
            let animation = animations
                .get(&segment.animation_id)
                .ok_or_else(|| format!("missing root animation {:08x}", segment.animation_id))?;
            if animation.id != segment.animation_id {
                return Err("root animation identity mismatch".into());
            }
            let frames = animation
                .position_frames
                .iter()
                .map(|frame| {
                    let q = frame.rotation;
                    let length = q.iter().map(|v| v * v).sum::<f32>();
                    if q[1].abs() > 0.0002
                        || q[2].abs() > 0.0002
                        || !(0.999..=1.001).contains(&length)
                    {
                        return Err(
                            "non-upright root animation requires full orientation playback"
                                .to_owned(),
                        );
                    }
                    let heading = (2.0 * q[3].atan2(q[0]) + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    Ok(RootFrame {
                        translation: Vec3::new(frame.origin[0], frame.origin[1], frame.origin[2])
                            * scale,
                        heading,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            segments.push(RootSegment {
                low: segment.low_frame,
                high: segment.high_frame,
                frame_count: animation.num_frames,
                framerate: segment.framerate,
                frames,
            });
        }
        RootCycle::prepare(segments).map_err(|e| format!("invalid root animation: {e:?}"))
    };
    Ok(std::sync::Arc::new(AnimatedLocomotion {
        profile,
        ready: cycle(0x41000003)?,
        walk: cycle(0x45000005)?,
        run: cycle(0x44000007)?,
    }))
}

pub fn prepare_locomotion(
    table: &MotionTable,
    style: u32,
) -> Result<bace_motion::LocomotionProfile, String> {
    use bace_motion::{LocomotionProfile, MotionPhysics};
    let values = |data: &MotionData| MotionPhysics {
        velocity: data
            .velocity
            .map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2])),
        omega: data
            .omega
            .map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2])),
    };
    let cycle = |motion: u32| {
        table
            .cycles
            .get(&(style.wrapping_shl(16) | (motion & 0xffffff)))
            .or_else(|| {
                table
                    .cycles
                    .get(&(table.default_style.wrapping_shl(16) | (motion & 0xffffff)))
            })
            .map(values)
            .ok_or_else(|| format!("missing locomotion cycle {motion:08x}"))
    };
    let modifier = |motion: u32| {
        table
            .modifiers
            .get(&(style.wrapping_shl(16) | (motion & 0xffffff)))
            .or_else(|| table.modifiers.get(&(motion & 0xffffff)))
            .map(values)
            .ok_or_else(|| format!("missing locomotion modifier {motion:08x}"))
    };
    Ok(LocomotionProfile {
        style,
        ready: cycle(0x41000003)?,
        walk: cycle(0x45000005)?,
        run: cycle(0x44000007)?,
        sidestep: modifier(0x6500000f)?,
        turn: modifier(0x6500000d)?,
    })
}

pub fn prepare_collision_shape(
    setup: &bace_dat::CollisionSetup,
    scale: f32,
) -> Result<std::sync::Arc<bace_physics::CollisionShape>, String> {
    use bace_physics::{CollisionCylinder, CollisionShape, CollisionSphere};
    if !scale.is_finite() || !(0.001..=100.0).contains(&scale) {
        return Err("invalid setup scale".into());
    }
    let spheres = if setup.spheres.is_empty() {
        // PhysicsObj.SetPosition uses the source dummy sphere at scale1.
        vec![CollisionSphere {
            center: Vec3::new(0.0, 0.0, 0.1),
            radius: 0.1,
        }]
    } else {
        setup
            .spheres
            .iter()
            .map(|s| CollisionSphere {
                center: Vec3::new(s.origin[0], s.origin[1], s.origin[2]) * scale,
                radius: s.radius * scale,
            })
            .collect()
    };
    let cylinders = setup
        .cylinders
        .iter()
        .map(|c| CollisionCylinder {
            base: Vec3::new(c.origin[0], c.origin[1], c.origin[2]) * scale,
            radius: c.radius * scale,
            height: c.height * scale,
        })
        .collect();
    CollisionShape::prepare(
        spheres,
        setup.step_up_height * scale,
        setup.step_down_height * scale,
    )
    .and_then(|s| s.with_cylinders(cylinders))
    .and_then(|s| s.with_nominal_dimensions(setup.radius * scale, setup.height * scale))
    .map(std::sync::Arc::new)
    .map_err(|e| e.to_string())
}

pub fn prepare_motion(
    data: &MotionData,
    animations: &BTreeMap<u32, Animation>,
) -> Result<PreparedMotion, String> {
    let mut segments = Vec::with_capacity(data.animations.len());
    for segment in &data.animations {
        let animation = animations
            .get(&segment.animation_id)
            .ok_or_else(|| format!("missing animation {:08x}", segment.animation_id))?;
        if animation.id != segment.animation_id
            || animation.frames.len() != animation.num_frames as usize
        {
            return Err("animation identity/frame mismatch".into());
        }
        let hooks = animation_hooks(animation);
        segments.push(MotionSegment {
            animation: animation.id,
            frame_count: animation.num_frames,
            low_frame: segment.low_frame,
            high_frame: segment.high_frame,
            frames_per_second: segment.framerate,
            hooks,
        });
    }
    let vector = |v: Option<[f32; 3]>| v.map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
    PreparedMotion::prepare(&segments, vector(data.velocity), vector(data.omega))
        .map_err(|e| format!("invalid motion: {e:?}"))
}
/// Exact pinned MotionTable.GetAnimData lookup, retaining missing-link failure.
pub fn transition_data(
    table: &MotionTable,
    style: u32,
    current: u32,
    motion: u32,
) -> Result<&MotionData, String> {
    let key = style.wrapping_shl(16) | (current & 0xfffff);
    let links = table.links.get(&key).ok_or("missing current motion link")?;
    links
        .get(&motion)
        .or_else(|| {
            table
                .links
                .get(&style.wrapping_shl(16))
                .and_then(|l| l.get(&motion))
        })
        .ok_or_else(|| "missing transition motion".into())
}
pub fn prepare_combat_maneuvers(
    table: &CombatManeuverTable,
    motions: &MotionTable,
    animations: &BTreeMap<u32, Animation>,
) -> Result<Vec<bace_gameplay_api::weapon_combat::PhysicalManeuver>, String> {
    use bace_gameplay_api::weapon_combat::{PhysicalAttackHook, PhysicalManeuver};
    if table.maneuvers.len() > 4096 {
        return Err("CMT preparation capacity".into());
    }
    table
        .maneuvers
        .iter()
        .map(|m| {
            let current = *motions
                .style_defaults
                .get(&m.style)
                .ok_or("missing stance default")?;
            let data = transition_data(motions, m.style, current, m.motion)?;
            let timeline = prepare_motion(data, animations)?;
            let hooks = timeline
                .hooks()
                .iter()
                .filter_map(|h| match h.payload {
                    MotionHookPayload::Attack { part, .. } => Some(PhysicalAttackHook {
                        seconds: h.seconds,
                        part,
                    }),
                    _ => None,
                })
                .collect();
            Ok(PhysicalManeuver {
                style: m.style,
                attack_type: m.attack_type,
                height: m.attack_height,
                minimum_skill: m.min_skill_level,
                motion: m.motion,
                duration: timeline.duration_seconds(),
                hooks,
            })
        })
        .collect()
}

fn animation_hooks(animation: &Animation) -> Vec<MotionHook> {
    let mut hooks = Vec::new();
    for (frame, value) in animation.frames.iter().enumerate() {
        for hook in &value.hooks {
            let payload = match &hook.payload {
                AnimationHookPayload::Attack(a) => MotionHookPayload::Attack {
                    part: a.part_index,
                    left: a.left,
                    right: a.right,
                    radius: a.radius,
                    height: a.height,
                },
                AnimationHookPayload::State(s) => MotionHookPayload::State(*s),
                AnimationHookPayload::Id(id) => MotionHookPayload::Id(*id),
                AnimationHookPayload::Empty => MotionHookPayload::Empty,
                AnimationHookPayload::ReplaceObject {
                    raw_part_index,
                    part_index,
                    object_id,
                } => MotionHookPayload::ReplaceObject {
                    raw_part_index: *raw_part_index,
                    part_index: *part_index,
                    object_id: *object_id,
                },
                AnimationHookPayload::Transition {
                    part,
                    start,
                    end,
                    time,
                } => MotionHookPayload::Transition {
                    part: *part,
                    start: *start,
                    end: *end,
                    time: *time,
                },
                AnimationHookPayload::Scale { end, time } => MotionHookPayload::Scale {
                    end: *end,
                    time: *time,
                },
                AnimationHookPayload::Particle {
                    emitter_info_id,
                    part_index,
                    offset,
                    emitter_id,
                } => MotionHookPayload::Particle {
                    emitter_info_id: *emitter_info_id,
                    part_index: *part_index,
                    origin: offset.origin,
                    rotation: offset.rotation,
                    emitter_id: *emitter_id,
                },
                AnimationHookPayload::CallPes { pes, pause } => MotionHookPayload::CallPes {
                    pes: *pes,
                    pause: *pause,
                },
                AnimationHookPayload::SoundTweaked {
                    sound_id,
                    priority,
                    probability,
                    volume,
                } => MotionHookPayload::SoundTweaked {
                    sound_id: *sound_id,
                    priority: *priority,
                    probability: *probability,
                    volume: *volume,
                },
                AnimationHookPayload::Omega(v) => MotionHookPayload::Omega(*v),
                AnimationHookPayload::TextureVelocity { part, uv } => {
                    MotionHookPayload::TextureVelocity {
                        part: *part,
                        uv: *uv,
                    }
                }
            };
            hooks.push(MotionHook {
                frame: frame as u32,
                kind: hook.kind,
                direction: hook.direction,
                payload,
            });
        }
    }
    hooks
}
