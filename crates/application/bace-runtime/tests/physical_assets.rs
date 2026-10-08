//! Approved real DAT physical actions/style links, without synthetic frame data.
use bace_content::{BodyPart, Property, WeenieV1};
use bace_dat::{Animation, CombatManeuverTable, DatArchive, MotionTable};
use bace_gameplay_api::weapon_combat::PhysicalSkill;
use bace_runtime::physical_assets::*;
use std::collections::{BTreeMap, BTreeSet};
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn normal_humanoid_actions_and_cast_return_use_actual_dat_style_links() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = DatArchive::open(path).unwrap();
    let table = MotionTable::decode(&dat.read(0x09000001).unwrap()).unwrap();
    let combat = CombatManeuverTable::decode(&dat.read(0x30000000).unwrap()).unwrap();
    let ids: BTreeSet<_> = table
        .links
        .values()
        .flat_map(|v| v.values())
        .chain(table.cycles.values())
        .flat_map(|d| d.animations.iter().map(|a| a.animation_id))
        .collect();
    let mut animations = BTreeMap::new();
    for id in ids {
        animations.insert(id, Animation::decode(&dat.read(id).unwrap()).unwrap());
    }
    let maneuvers =
        bace_runtime::world_admission::prepare_combat_maneuvers(&combat, &table, &animations)
            .unwrap();
    let mut source = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "physical-dat-projection".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    source.properties.body_parts = vec![Property {
        id: 0,
        value: BodyPart {
            d_type: 4,
            d_val: 3,
            d_var: 0.2,
            llf: 1.,
            llb: 1.,
            lrf: 1.,
            lrb: 1.,
            mlf: 1.,
            mlb: 1.,
            mrf: 1.,
            mrb: 1.,
            hlf: 1.,
            hlb: 1.,
            hrf: 1.,
            hrb: 1.,
            ..Default::default()
        },
    }];
    let skills: Vec<_> = [6, 7, 44, 45, 46, 48, 49]
        .into_iter()
        .map(|s| {
            (
                s,
                PhysicalSkill {
                    advancement: 2,
                    current: 200,
                },
            )
        })
        .collect();
    let profile =
        bace_combat::preparation::prepare_physical(bace_combat::preparation::PhysicalPreparation {
            actor: 1,
            revision: 1,
            content_hash: [1; 32],
            player: true,
            weenie: &source,
            equipment: &[],
            skills: &skills,
            attributes: [100; 6],
            base_attributes: [100; 6],
            qualities: &[],
            enchantments_complete: true,
            maneuvers,
            height: 1.8,
            missile: None,
            melee_defense_modifier: 1.0,
            missile_defense_modifier: 1.0,
        })
        .unwrap();
    let chains = prepare_physical_motion_assets(
        &profile,
        PhysicalMotionAssets {
            table: &table,
            animations: &animations,
            current_motion: 0x41000003,
            current_speed: 1.0,
            scale: 1.0,
            modifiers: &[],
        },
    )
    .unwrap();
    assert!(!chains.is_empty());
    assert!(
        chains.iter().any(|(_, _, c)| c
            .source_transition()
            .is_some_and(|s| s.before.style == 0x80000049 && s.after.style == profile.style)),
        "actual casting->physical style transition"
    );
    for (motion, _, chain) in &chains {
        assert!(chain.stop_chain().is_some());
        assert!(chain.source_transition().is_some());
        assert_eq!(*motion, chain.motion);
        if motion & 0x10000000 != 0 {
            for speed in [0.8, 1.0, 1.3, 2.25] {
                let before = chain.source_transition().unwrap().before;
                let fresh = bace_runtime::world_admission::prepare_motion_chain(
                    &table,
                    &animations,
                    bace_runtime::world_admission::MotionChainRequest {
                        style: before.style,
                        current_motion: before.substate,
                        current_speed: before.speed,
                        action: *motion,
                        action_speed: speed,
                        scale: 1.0,
                        modifiers: &[],
                    },
                )
                .unwrap();
                assert_eq!(
                    chain.retime_action(speed).unwrap(),
                    *fresh,
                    "fresh source resolution must match role-aware retiming {motion:08x} at {speed}"
                );
            }
        }
    }
    let prior_style = *table
        .style_defaults
        .keys()
        .find(|&&style| {
            style != profile.style
                && ![0x8000003d, 0x80000049].contains(&style)
                && [0x45000005u32, 0x44000007].into_iter().all(|motion| {
                    table
                        .cycles
                        .contains_key(&(style.wrapping_shl(16) | (motion & 0xffffff)))
                })
        })
        .expect("actual prior weapon locomotion style");
    let replacement = prepare_physical_motion_assets_with_previous_style(
        &profile,
        PhysicalMotionAssets {
            table: &table,
            animations: &animations,
            current_motion: 0x41000003,
            current_speed: 1.,
            scale: 1.,
            modifiers: &[],
        },
        Some(prior_style),
    )
    .unwrap();
    assert!(replacement.len() <= 128);
    for substate in [0x45000005, 0x44000007] {
        assert!(
            replacement
                .iter()
                .any(|(_, _, chain)| chain.source_transition().is_some_and(
                    |transition| transition.before.style == prior_style
                        && transition.before.substate == substate
                        && transition.after.style == profile.style
                )),
            "moving previous equipment style must enter new style through DAT"
        );
    }
    eprintln!(
        "actual DAT equipment closure with previous style {prior_style:08x}: {}",
        replacement.len()
    );
    let styles = prepare_locomotion_styles(&table, &animations, profile.style, 1.0).unwrap();
    let deaths = prepare_death_motion_assets(&table, &animations, &styles, 1.0).unwrap();
    assert_eq!(deaths.len(), styles.len() * 4);
    let peace = styles
        .iter()
        .find(|s| s.profile.style == 0x8000003d)
        .unwrap()
        .clone();
    let magic = styles
        .iter()
        .find(|s| s.profile.style == 0x80000049)
        .unwrap()
        .clone();
    for (forward, run, substate, rate) in [
        (1.0f32, true, 0x44000007, 1.7f32),
        (1.0, false, 0x45000005, 1.0),
        (-1.0, false, 0x45000005, -0.65),
    ] {
        let raw = chains
            .iter()
            .find(|(_, _, c)| {
                c.source_transition().is_some_and(|s| {
                    s.before.style == 0x8000003d
                        && s.before.substate == substate
                        && s.before.speed.is_sign_negative() == rate.is_sign_negative()
                        && s.after.style == 0x80000049
                })
            })
            .unwrap()
            .2
            .clone();
        let chain = std::sync::Arc::new(raw.retime_current_action(rate, 1.0).unwrap());
        let fresh = bace_runtime::world_admission::prepare_motion_chain(
            &table,
            &animations,
            bace_runtime::world_admission::MotionChainRequest {
                style: 0x8000003d,
                current_motion: substate,
                current_speed: rate,
                action: 0x80000049,
                action_speed: 1.0,
                scale: 1.0,
                modifiers: &[],
            },
        )
        .unwrap();
        assert_eq!(*chain, *fresh);
        let cell = bace_types::CellId(0x10100001);
        let actor = bace_types::EntityId(99);
        let scene = bace_physics::SyntheticScene::new(
            0.0,
            bace_geometry::Aabb::new(
                bace_geometry::Vec3::new(-100., -100., -1.),
                bace_geometry::Vec3::new(100., 100., 100.),
            )
            .unwrap(),
            vec![],
        )
        .unwrap();
        let mut body = bace_physics::Body::spawn(
            &scene,
            bace_geometry::Vec3::new(0., 0., 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 10.,
                jump_impulse: 5.,
            },
        )
        .unwrap();
        body.submit_animated_locomotion(
            0,
            1,
            peace.clone(),
            bace_motion::LocomotionControls {
                forward,
                run,
                sidestep: 0.25,
                turn: 0.25,
            },
            1.7,
        )
        .unwrap();
        let mut world = bace_world::World::default();
        world.register_scene(cell, scene).unwrap();
        world
            .insert(bace_entity::Actor {
                id: actor,
                cell,
                body,
            })
            .unwrap();
        world
            .register_locomotion_styles(actor, styles.clone())
            .unwrap();
        assert_eq!(world.source_motion_state(actor).unwrap().substate, substate);
        let clear = chain.clears_style_modifiers();
        world
            .begin_motion(
                actor,
                bace_motion::MotionToken {
                    domain: bace_motion::MotionDomain::Casting,
                    owner: 1,
                    sequence: 1,
                },
                chain,
            )
            .unwrap();
        let (style, drive) = world.body(actor).unwrap().locomotion_projection().unwrap();
        assert_eq!(style, magic.profile.style);
        assert_eq!(drive.forward_motion, 0x41000003);
        assert_eq!(drive.forward_rate, 1.0);
        assert_eq!(drive.side_rate == 0.0, clear);
        assert_eq!(drive.turn_rate == 0.0, clear);
        for _ in 0..80 {
            world.tick().unwrap();
        }
        assert_eq!(world.source_motion_state(actor).unwrap().style, 0x80000049);
        world.register_death_motions(actor, deaths.clone()).unwrap();
        world
            .register_combatant(
                actor,
                bace_entity::Combatant::new(bace_entity::CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 1,
                    melee_range: 1.,
                    attack_duration: 1.,
                    strike_offsets: vec![0.5],
                    player: true,
                })
                .unwrap(),
            )
            .unwrap();
        world.combatant_mut(actor).unwrap().damage(10).unwrap();
        world.tick().unwrap();
        let token = world.begin_death_motion(actor).unwrap();
        let epoch = world.body(actor).unwrap().accepted().epoch();
        assert_eq!(token.domain, bace_motion::MotionDomain::Death);
        assert!(!world.death_motion_complete(actor, token, epoch));
        for _ in 0..180 {
            world.tick().unwrap();
            if world.death_motion_complete(actor, token, epoch) {
                break;
            }
        }
        assert!(world.death_motion_complete(actor, token, epoch));
        let view = world.accepted_object_view(actor).unwrap();
        let motion = view.motion.unwrap().unwrap();
        assert_eq!(motion.forward_motion, 0x40000011);
        assert!(motion.actions.is_empty());
        assert!(!world.body(actor).unwrap().has_locomotion_intent());
    }
    eprintln!(
        "actual DAT physical actions/style entries: {}",
        chains.len()
    );
}
