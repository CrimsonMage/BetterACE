//! Geometry-owner lifecycle regressions; synthetic authored assets, no client capture.
use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_geometry::Vec3;
use bace_motion::*;
use bace_physics::*;
use bace_types::{CellId, EntityId};
use bace_world::World;
use std::sync::Arc;
fn fixture(airborne: bool) -> (World, Arc<AnimatedLocomotion>) {
    let cell = CellId(0x100);
    let region = Arc::new(
        GeometryRegion::prepare(vec![GeometryCell {
            id: cell.0,
            restriction: None,
            terrain: false,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![
                CollisionPlane {
                    normal: Vec3::new(1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(-1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., 1., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., -1., 0.),
                    distance: 20.,
                },
            ],
            faces: vec![CollisionFace {
                polygon: GdlePolygon::prepare(vec![
                    Vec3::new(-100., -100., 0.),
                    Vec3::new(100., -100., 0.),
                    Vec3::new(100., 100., 0.),
                    Vec3::new(-100., 100., 0.),
                ])
                .unwrap(),
                two_sided: true,
                object: None,
            }],
            portals: vec![],
        }])
        .unwrap(),
    );
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 0.5),
                radius: 0.5,
            }],
            0.,
            0.2,
        )
        .unwrap(),
    );
    let zero = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let cycle = || {
        RootCycle::prepare(vec![RootSegment {
            low: 0,
            high: 0,
            frame_count: 1,
            framerate: 30.,
            frames: vec![RootFrame::default()],
        }])
        .unwrap()
    };
    let style = Arc::new(AnimatedLocomotion {
        profile: LocomotionProfile {
            style: 0x80000049,
            ready: zero,
            walk: MotionPhysics {
                velocity: Vec3::new(0., 2., 0.),
                ..zero
            },
            run: MotionPhysics {
                velocity: Vec3::new(0., 4., 0.),
                ..zero
            },
            sidestep: zero,
            turn: zero,
        },
        ready: cycle(),
        walk: cycle(),
        run: cycle(),
    });
    let mut body = Body::spawn_geometry(
        &region,
        GeometrySpawn {
            cell: cell.0,
            position: Vec3::new(0., 0., if airborne { 4. } else { 0. }),
            shape,
            capabilities: Capabilities {
                speed: 5.,
                jump_impulse: 5.,
            },
            heading: 0.,
            maximum_turn_rate: 3.,
        },
    )
    .unwrap();
    body.submit_animated_locomotion(
        0,
        1,
        style.clone(),
        LocomotionControls {
            forward: 1.,
            run: true,
            ..Default::default()
        },
        1.5,
    )
    .unwrap();
    let mut world = World::default();
    world.install_geometry(region).unwrap();
    world
        .insert(Actor {
            id: EntityId(1),
            cell,
            body,
        })
        .unwrap();
    world
        .register_combatant(
            EntityId(1),
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 1.,
                attack_duration: 1.,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap(),
        )
        .unwrap();
    world
        .register_locomotion_styles(EntityId(1), vec![style.clone()])
        .unwrap();
    (world, style)
}
#[test]
fn death_stops_previous_run_while_authoritative_gravity_continues() {
    let (mut world, _) = fixture(true);
    world.tick().unwrap();
    let before = world.body(EntityId(1)).unwrap().accepted();
    assert!(world.body(EntityId(1)).unwrap().has_locomotion_intent());
    assert!(!before.grounded());
    world
        .combatant_mut(EntityId(1))
        .unwrap()
        .damage(100)
        .unwrap();
    world.tick().unwrap();
    let after = world.body(EntityId(1)).unwrap().accepted();
    assert_eq!(after.position().y, before.position().y);
    assert!(after.position().z < before.position().z);
    assert!(!world.body(EntityId(1)).unwrap().has_locomotion_intent());
    for _ in 0..90 {
        world.tick().unwrap();
    }
    assert_eq!(
        world.body(EntityId(1)).unwrap().accepted().position().y,
        before.position().y
    );
}
#[test]
fn portal_stop_and_explicit_teleport_restore_actual_style_ready_without_old_controls() {
    let (mut world, style) = fixture(false);
    world.tick().unwrap();
    world.begin_portal_transit(&[(EntityId(1), 0)], 9).unwrap();
    assert!(
        world
            .body(EntityId(1))
            .unwrap()
            .animated_locomotion()
            .is_none()
    );
    world
        .teleport(EntityId(1), CellId(0x100), Vec3::new(5., 5., 0.))
        .unwrap();
    world.finish_portal_transit(EntityId(1), 9).unwrap();
    let body = world.body(EntityId(1)).unwrap();
    assert!(Arc::ptr_eq(body.animated_locomotion().unwrap(), &style));
    assert_eq!(body.locomotion_run_rate(), Some(1.5));
    let (style_id, drive) = body.locomotion_projection().unwrap();
    assert_eq!(style_id, 0x80000049);
    assert_eq!(drive.forward_motion, 0x41000003);
    assert!(!body.has_locomotion_intent());
    assert_eq!(body.accepted().epoch(), 1);
    assert!(
        world
            .reset_locomotion_style(EntityId(1), 0, style_id, 1.)
            .is_err()
    );
    world
        .reset_locomotion_style(EntityId(1), 1, style_id, 2.)
        .unwrap();
    for _ in 0..3 {
        world.tick().unwrap();
    }
    let view = world.accepted_object_view(EntityId(1)).unwrap();
    assert_eq!(view.position[0], 5.);
    assert_eq!(view.position[1], 5.);
    assert_eq!(view.motion.unwrap().unwrap().style, style_id);
}
#[test]
fn death_queues_after_positive_action_preserves_callback_and_finishes_on_trusted_cursor() {
    let (mut world, style) = fixture(false);
    let actor = EntityId(1);
    world
        .body_mut(actor)
        .unwrap()
        .submit_animated_locomotion(0, 2, style, LocomotionControls::default(), 1.)
        .unwrap();
    let before = SourceMotionState {
        style: 0x80000049,
        substate: 0x41000003,
        speed: 1.,
    };
    let after = SourceMotionState {
        substate: 0x40000011,
        ..before
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let clip = |id, n| ExecutionClip {
        animation: id,
        frame_count: n,
        low: 0,
        high: -1,
        framerate: 30.,
        frames: vec![RootFrame::default(); n as usize].into(),
        hooks: vec![].into(),
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![clip(1, 1)], 0, 0, physics, 0x41000003, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before,
                after: before,
                continues_cycle: false,
            })
            .unwrap(),
    );
    let dead = Arc::new(
        PreparedMotionChain::prepare(vec![clip(2, 6), clip(3, 1)], 1, 1, physics, 0x40000011, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before,
                after,
                continues_cycle: false,
            })
            .unwrap(),
    );
    world
        .register_death_motions(
            actor,
            vec![PreparedDeathMotion {
                stop: stop.clone(),
                dead,
            }],
        )
        .unwrap();
    let action = Arc::new(
        PreparedMotionChain::prepare(vec![clip(4, 6), clip(1, 1)], 1, 1, physics, 0x10000063, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before,
                after: before,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop)
            .unwrap(),
    );
    let old = MotionToken {
        domain: MotionDomain::Physical,
        owner: 7,
        sequence: 1,
    };
    world.begin_motion(actor, old, action).unwrap();
    world.tick().unwrap();
    world.combatant_mut(actor).unwrap().damage(100).unwrap();
    world.tick().unwrap();
    let token = world.begin_death_motion(actor).unwrap();
    assert_eq!(token.domain, MotionDomain::Death);
    assert!(!world.death_motion_complete(actor, token, 0));
    let mut old_done = 0;
    for _ in 0..30 {
        world.tick().unwrap();
        while let Some(event) = world.take_motion_event() {
            assert_ne!(event.token.domain, MotionDomain::Death);
            if event.token == old && matches!(event.event, MotionExecutionEvent::Completed) {
                old_done += 1;
            }
        }
    }
    assert_eq!(old_done, 1);
    assert!(world.death_motion_complete(actor, token, 0));
    assert!(!world.death_motion_complete(actor, token, 1));
    let motion = world
        .accepted_object_view(actor)
        .unwrap()
        .motion
        .unwrap()
        .unwrap();
    assert_eq!(motion.forward_motion, 0x40000011);
    assert!(motion.actions.is_empty());
    assert!(!world.body(actor).unwrap().has_locomotion_intent());
}
#[test]
fn equipment_death_cache_keeps_current_old_style_until_the_authored_transition() {
    let (mut world, style) = fixture(false);
    let actor = EntityId(1);
    world
        .body_mut(actor)
        .unwrap()
        .adopt_animated_style(style, true)
        .unwrap();
    let row = |style| {
        let before = SourceMotionState {
            style,
            substate: 0x41000003,
            speed: 1.,
        };
        let physics = MotionPhysics {
            velocity: Vec3::ZERO,
            omega: Vec3::ZERO,
        };
        let clip = |n| ExecutionClip {
            animation: 1,
            frame_count: n,
            low: 0,
            high: -1,
            framerate: 30.,
            frames: vec![RootFrame::default(); n as usize].into(),
            hooks: vec![].into(),
        };
        let stop = PreparedMotionChain::prepare(vec![clip(1)], 0, 0, physics, 0x41000003, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before,
                after: before,
                continues_cycle: false,
            })
            .unwrap();
        let dead =
            PreparedMotionChain::prepare(vec![clip(4), clip(1)], 1, 1, physics, 0x40000011, 1.)
                .unwrap()
                .with_source_transition(SourceMotionTransition {
                    before,
                    after: SourceMotionState {
                        substate: 0x40000011,
                        ..before
                    },
                    continues_cycle: false,
                })
                .unwrap();
        PreparedDeathMotion {
            stop: Arc::new(stop),
            dead: Arc::new(dead),
        }
    };
    world
        .register_death_motions(actor, vec![row(0x80000049)])
        .unwrap();
    world
        .validate_death_motions(actor, &[row(0x8000003d)])
        .unwrap();
    world
        .register_death_motions(actor, vec![row(0x8000003d)])
        .unwrap();
    world.combatant_mut(actor).unwrap().damage(100).unwrap();
    world.tick().unwrap();
    let token = world.begin_death_motion(actor).unwrap();
    for _ in 0..30 {
        world.tick().unwrap();
    }
    assert!(
        world.death_motion_complete(actor, token, 0),
        "token={token:?} current={:?} state={:?}",
        world.source_motion_token(actor),
        world.source_motion_state(actor)
    );
    assert_eq!(world.source_motion_state(actor).unwrap().style, 0x80000049);
}
