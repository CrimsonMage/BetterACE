use bace_gameplay_api::weapon_combat::*;
fn prepared(player: bool) -> PhysicalCombatProfile {
    let weapon = PhysicalWeapon {
        entity: if player { 100 } else { 200 },
        revision: 1,
        style: 2,
        attack_type: 6,
        damage_type: 3,
        damage: 20.0,
        variance: 0.0,
        skill: 44,
        attack_time: 0,
        encumbrance: 0,
        offense: 1.0,
        imbues: 0,
        biting: 0.0,
        crushing: 0.0,
        slayer_type: 0,
        slayer_bonus: 0.0,
        cleave_targets: 1,
        ignore_magic_armor: false,
        ignore_magic_resistance: false,
        armor_cleaving: false,
        resistance_cleaving: None,
        proc_spell: None,
        proc_chance: 0.0,
    };
    let armor = PhysicalBodyDefense {
        part: 0,
        hit_weights: [1.0; 12],
        armor: [quality(0.0); 8],
    };
    PhysicalCombatProfile {
        equipment: vec![PhysicalEquipmentStamp {
            entity: if player { 100 } else { 200 },
            revision: 1,
            location: 0x100000,
        }],
        revision: 1,
        content_hash: [7; 32],
        player,
        creature_type: 1,
        pk: PkStatus::Npk,
        attackable: true,
        immune: false,
        lifestone_protected: false,
        style: 0x80000040,
        main: Some(weapon),
        offhand: None,
        launcher: None,
        ammunition: None,
        gloves: None,
        boots: None,
        body_attacks: vec![],
        maneuvers: vec![PhysicalManeuver {
            style: 0x80000040,
            attack_type: 4,
            height: 2,
            minimum_skill: 0,
            motion: 0x10000063,
            duration: 0.2,
            hooks: vec![
                PhysicalAttackHook {
                    seconds: 0.05,
                    part: 0,
                },
                PhysicalAttackHook {
                    seconds: 0.1,
                    part: 0,
                },
            ],
        }],
        skills: vec![
            (
                6,
                PhysicalSkill {
                    advancement: 2,
                    current: 100,
                },
            ),
            (
                44,
                PhysicalSkill {
                    advancement: 3,
                    current: 300,
                },
            ),
        ],
        strength: 100,
        coordination: 100,
        quickness: 100,
        base_strength: 0,
        base_endurance: 0,
        melee_defense_modifier: 1.0,
        missile_defense_modifier: 1.0,
        armor: vec![armor],
        resistances: [PhysicalResistance {
            quality: quality(1.0),
            augmentation: 0,
        }; 8],
        shield_encumbrance: 0,
        shield_placement: true,
        armor_layers: vec![],
        ignore_shield: 0.0,
        shield: None,
        shield_skill: PhysicalSkill {
            advancement: 0,
            current: 0,
        },
        ratings: PhysicalRatings::default(),
        critical_defense: false,
        range: 3.0,
        height: 1.0,
        missile: None,
    }
}
fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    }
}

pub(super) fn profile() -> std::sync::Arc<PhysicalCombatProfile> {
    let mut p = prepared(false);
    p.equipment.clear();
    p.main = None;
    std::sync::Arc::new(p)
}
/// Explicit prepared animation fixture for resource/adoption tests. Runtime uses
/// the verified DAT cold closure; this synthetic program does not claim parity.
type FixtureMotionAssets = (
    Vec<(u32, f32, std::sync::Arc<bace_motion::PreparedMotionChain>)>,
    Vec<std::sync::Arc<bace_motion::AnimatedLocomotion>>,
);
pub(super) fn motion_assets() -> FixtureMotionAssets {
    use bace_motion::*;
    use std::sync::Arc;
    let (_, _, _, geometry, _) = super::fixture::physical_fixture();
    let mut physical = (*geometry.locomotion).clone();
    physical.profile.style = 0x80000040;
    let source = SourceMotionState {
        style: 0x80000040,
        substate: 0x41000003,
        speed: 1.0,
    };
    let clip = ExecutionClip {
        animation: 0x03000001,
        frame_count: 2,
        low: 0,
        high: 1,
        framerate: 30.,
        frames: vec![RootFrame::default(); 2].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: bace_geometry::Vec3::ZERO,
        omega: bace_geometry::Vec3::ZERO,
    };
    let stop = PreparedMotionChain::prepare(vec![clip.clone()], 0, 0, physics, 0x41000003, 1.0)
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: source,
            after: source,
            continues_cycle: false,
        })
        .unwrap();
    let action =
        PreparedMotionChain::prepare(vec![clip.clone(), clip], 1, 1, physics, 0x10000063, 1.0)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: source,
                after: source,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(Arc::new(stop))
            .unwrap();
    (
        vec![(0x10000063, 1.0, Arc::new(action))],
        vec![geometry.locomotion, Arc::new(physical)],
    )
}
