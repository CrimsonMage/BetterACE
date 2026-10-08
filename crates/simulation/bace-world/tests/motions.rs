use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_geometry::{Aabb, Vec3};
use bace_motion::{
    Capabilities, ExecutionClip, MotionDomain, MotionExecutionEvent, MotionHook, MotionHookPayload,
    MotionPhysics, MotionToken, PreparedMotionChain, RootFrame,
};
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::World;
use std::sync::Arc;
fn program(root: f32) -> Arc<PreparedMotionChain> {
    let clip = |id, count, hooks: Vec<MotionHook>| ExecutionClip {
        animation: id,
        frame_count: count,
        low: 0,
        high: -1,
        framerate: 30.0,
        frames: vec![
            RootFrame {
                translation: Vec3::new(root, 0.0, 0.0),
                heading: 0.0
            };
            count as usize
        ]
        .into(),
        hooks: hooks.into(),
    };
    Arc::new(
        PreparedMotionChain::prepare(
            vec![
                clip(
                    3,
                    4,
                    vec![MotionHook {
                        frame: 1,
                        kind: 1,
                        direction: 0,
                        payload: MotionHookPayload::Id(7),
                    }],
                ),
                clip(4, 2, vec![]),
            ],
            1,
            1,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            0x13000132,
            2.0,
        )
        .unwrap(),
    )
}
fn world() -> World {
    let scene = SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        vec![],
    )
    .unwrap();
    let body = Body::spawn(
        &scene,
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 3.0,
            jump_impulse: 1.0,
        },
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene).unwrap();
    world
        .insert(Actor {
            id: EntityId(1),
            cell: CellId(1),
            body,
        })
        .unwrap();
    world
        .register_combatant(
            EntityId(1),
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 1.0,
                attack_duration: 1.0,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap(),
        )
        .unwrap();
    world
}
fn token(sequence: u64) -> MotionToken {
    MotionToken {
        domain: MotionDomain::Casting,
        owner: 1,
        sequence,
    }
}
#[test]
fn source_sequence_completion_preserves_authorized_rootless_slide_and_drains_before_retire() {
    let mut world = world();
    world
        .body_mut(EntityId(1))
        .unwrap()
        .submit_intent(
            0,
            1,
            bace_motion::MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
        )
        .unwrap();
    world
        .begin_motion(EntityId(1), token(1), program(0.0))
        .unwrap();
    assert!(!world.can_retire_actor_motion(EntityId(1)));
    for _ in 0..4 {
        world.tick().unwrap();
    }
    assert!(world.body(EntityId(1)).unwrap().accepted().position().x > 0.39);
    assert!(!world.motion_busy(EntityId(1)));
    assert!(world.has_motion_state());
    assert!(world.retire_actor_motion(EntityId(1)).is_err());
    let mut completed = 0;
    let mut hooks = 0;
    while let Some(event) = world.take_motion_event_for(MotionDomain::Casting) {
        assert_eq!(event.token, token(1));
        match event.event {
            MotionExecutionEvent::Completed => completed += 1,
            MotionExecutionEvent::Hook { .. } => hooks += 1,
            _ => panic!(),
        }
    }
    assert_eq!(completed, 1);
    assert_eq!(hooks, 1);
    for _ in 0..6 {
        world.tick().unwrap();
    }
    assert!(world.take_motion_event().is_none());
    assert!(world.can_retire_actor_motion(EntityId(1)));
    // A new action replaces the retained idle cycle, never fabricates completion.
    world
        .begin_motion(EntityId(1), token(2), program(0.0))
        .unwrap();
    assert!(world.motion_busy(EntityId(1)));
    assert!(world.take_motion_event().is_none());
    world.cancel_motion(EntityId(1), token(2)).unwrap();
    assert!(world.retire_actor_motion(EntityId(1)).is_err());
    assert!(matches!(
        world.take_motion_event().unwrap().event,
        MotionExecutionEvent::Cancelled
    ));
    world.retire_actor_motion(EntityId(1)).unwrap();
    assert!(!world.has_motion_state());
}
#[test]
fn unsupported_nonzero_root_slide_rejects_before_sequence_or_pose_mutation() {
    let mut world = world();
    world
        .body_mut(EntityId(1))
        .unwrap()
        .submit_intent(
            0,
            1,
            bace_motion::MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
        )
        .unwrap();
    let before = world.body(EntityId(1)).unwrap().accepted();
    assert!(
        world
            .begin_motion(EntityId(1), token(1), program(0.1))
            .is_err()
    );
    assert_eq!(world.body(EntityId(1)).unwrap().accepted(), before);
    assert!(!world.has_motion_state());
    world.body_mut(EntityId(1)).unwrap().stop_motion();
    world
        .begin_motion(EntityId(1), token(1), program(0.1))
        .unwrap();
}
fn source_program(continuation: bool) -> Arc<PreparedMotionChain> {
    use bace_motion::{SourceMotionState as S, SourceMotionTransition as T};
    let ready = S {
        style: 0x80000049,
        substate: 0x41000003,
        speed: 1.0,
    };
    let pose = S {
        substate: 0x4000002f,
        speed: 2.0,
        ..ready
    };
    let original = program(0.0);
    let mut cyclic = original.clips()[0].clone();
    cyclic.animation = 10;
    cyclic.framerate = if continuation { 15.0 } else { 30.0 };
    cyclic.hooks = vec![MotionHook {
        frame: 2,
        kind: 1,
        direction: 0,
        payload: MotionHookPayload::Id(99),
    }]
    .into();
    let after = if continuation {
        S { speed: 1.0, ..pose }
    } else {
        pose
    };
    let mut idle = cyclic.clone();
    idle.animation = 11;
    idle.framerate = 30.0;
    idle.hooks = Vec::new().into();
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![idle], 0, 0, original.physics, ready.substate, 1.0)
            .unwrap()
            .with_source_transition(T {
                before: after,
                after: ready,
                continues_cycle: false,
            })
            .unwrap(),
    );
    Arc::new(
        PreparedMotionChain::prepare(
            vec![cyclic],
            0,
            0,
            original.physics,
            pose.substate,
            after.speed,
        )
        .unwrap()
        .with_source_transition(T {
            before: if continuation { pose } else { ready },
            after,
            continues_cycle: continuation,
        })
        .unwrap()
        .with_stop_chain(stop)
        .unwrap(),
    )
}
#[test]
fn repeated_substate_keeps_live_phase_and_endcast_restores_ready_for_next_cast() {
    let mut world = world();
    world
        .begin_motion(EntityId(1), token(1), source_program(false))
        .unwrap();
    world.tick().unwrap();
    assert_eq!(
        world.take_motion_event().unwrap().event,
        MotionExecutionEvent::Completed
    );
    world
        .begin_motion(EntityId(1), token(2), source_program(true))
        .unwrap();
    let mut hook_tick = None;
    let mut completions = 0;
    for tick in 1..=4 {
        world.tick().unwrap();
        while let Some(e) = world.take_motion_event() {
            assert_eq!(e.token, token(2));
            match e.event {
                MotionExecutionEvent::Hook { frame: 2, .. } => hook_tick = Some(tick),
                MotionExecutionEvent::Completed => completions += 1,
                _ => panic!(),
            }
        }
    }
    assert_eq!(completions, 1);
    assert_eq!(hook_tick, Some(4));
    world.end_cast_motion(EntityId(1), 1).unwrap();
    world.end_cast_motion(EntityId(1), 1).unwrap();
    assert_eq!(
        world.source_motion_state(EntityId(1)).unwrap().substate,
        0x41000003
    );
    world.tick().unwrap();
    assert_eq!(world.take_motion_event().unwrap().token, token(3));
    world
        .begin_motion(
            EntityId(1),
            MotionToken {
                owner: 2,
                sequence: 1,
                ..token(1)
            },
            source_program(false),
        )
        .unwrap();
}
#[test]
fn cancelled_cast_retains_active_links_and_original_callback_before_ready_callback() {
    use bace_motion::{SourceMotionState as S, SourceMotionTransition as T};
    let ready = S {
        style: 0x80000049,
        substate: 0x41000003,
        speed: 1.0,
    };
    let original = program(0.0);
    let stop = Arc::new(
        PreparedMotionChain::prepare(
            vec![original.clips()[1].clone()],
            0,
            0,
            original.physics,
            ready.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(T {
            before: ready,
            after: ready,
            continues_cycle: true,
        })
        .unwrap(),
    );
    let chain = Arc::new(
        (*original)
            .clone()
            .with_source_transition(T {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop)
            .unwrap(),
    );
    let mut world = world();
    world.begin_motion(EntityId(1), token(1), chain).unwrap();
    world.tick().unwrap();
    world.end_cast_motion(EntityId(1), 1).unwrap();
    assert!(world.motion_busy(EntityId(1)));
    assert!(
        world
            .begin_motion(
                EntityId(1),
                MotionToken {
                    owner: 2,
                    ..token(1)
                },
                source_program(false)
            )
            .is_err()
    );
    let mut callbacks = Vec::new();
    for _ in 0..3 {
        world.tick().unwrap();
        while let Some(e) = world.take_motion_event() {
            if e.event == MotionExecutionEvent::Completed {
                callbacks.push(e.token);
            }
        }
    }
    assert_eq!(callbacks, vec![token(1), token(2)]);
    assert!(!world.motion_busy(EntityId(1)));
    world
        .begin_motion(
            EntityId(1),
            MotionToken {
                owner: 2,
                ..token(1)
            },
            source_program(false),
        )
        .unwrap();
}
#[test]
fn actor_retirement_cancels_every_queued_token_without_losing_the_old_action() {
    use bace_motion::{SourceMotionState as S, SourceMotionTransition as T};
    let ready = S {
        style: 0x80000049,
        substate: 0x41000003,
        speed: 1.0,
    };
    let original = program(0.0);
    let stop = Arc::new(
        PreparedMotionChain::prepare(
            vec![original.clips()[1].clone()],
            0,
            0,
            original.physics,
            ready.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(T {
            before: ready,
            after: ready,
            continues_cycle: true,
        })
        .unwrap(),
    );
    let chain = Arc::new(
        (*original)
            .clone()
            .with_source_transition(T {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop)
            .unwrap(),
    );
    let mut world = world();
    world.begin_motion(EntityId(1), token(1), chain).unwrap();
    world.end_cast_motion(EntityId(1), 1).unwrap();
    world.remove(EntityId(1)).unwrap();
    world.tick().unwrap();
    for expected in [token(1), token(2)] {
        let event = world.take_motion_event().unwrap();
        assert_eq!(event.token, expected);
        assert_eq!(event.event, MotionExecutionEvent::Cancelled);
    }
    assert!(!world.has_motion_state());
}
#[test]
fn cancelled_substate_links_do_not_block_new_cast_but_its_action_callback_does() {
    use bace_motion::{SourceMotionState as S, SourceMotionTransition as T};
    let ready = S {
        style: 0x80000049,
        substate: 0x41000003,
        speed: 1.0,
    };
    let pose = S {
        substate: 0x4000002f,
        ..ready
    };
    let original = program(0.0);
    let stop = Arc::new(
        PreparedMotionChain::prepare(
            original.clips().to_vec(),
            1,
            1,
            original.physics,
            ready.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(T {
            before: pose,
            after: ready,
            continues_cycle: false,
        })
        .unwrap(),
    );
    let substate = Arc::new(
        PreparedMotionChain::prepare(
            original.clips().to_vec(),
            1,
            1,
            original.physics,
            pose.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(T {
            before: ready,
            after: pose,
            continues_cycle: false,
        })
        .unwrap()
        .with_stop_chain(stop)
        .unwrap(),
    );
    let action = Arc::new(
        (*original)
            .clone()
            .with_source_transition(T {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap(),
    );
    let mut world = world();
    world.begin_motion(EntityId(1), token(1), substate).unwrap();
    world.tick().unwrap();
    world.end_cast_motion(EntityId(1), 1).unwrap();
    assert!(world.motion_busy(EntityId(1)));
    assert!(!world.has_pending_action_motion(EntityId(1)));
    let new_token = MotionToken {
        owner: 2,
        sequence: 1,
        ..token(1)
    };
    world
        .begin_motion(EntityId(1), new_token, action.clone())
        .unwrap();
    assert!(world.has_pending_action_motion(EntityId(1)));
    assert!(
        world
            .begin_motion(
                EntityId(1),
                MotionToken {
                    owner: 3,
                    ..new_token
                },
                action
            )
            .is_err()
    );
    let mut completed = Vec::new();
    for _ in 0..12 {
        world.tick().unwrap();
        while let Some(event) = world.take_motion_event() {
            if event.event == MotionExecutionEvent::Completed {
                completed.push(event.token);
            }
        }
    }
    assert_eq!(completed, vec![token(1), token(2), new_token]);
    assert!(!world.has_pending_action_motion(EntityId(1)));
}
#[test]
fn zero_time_callback_drain_requires_consumed_server_completion_and_preserves_pose() {
    use bace_motion::{SourceMotionState as S, SourceMotionTransition as T};
    let mut world = world();
    world
        .begin_motion(EntityId(1), token(1), source_program(false))
        .unwrap();
    assert!(
        world
            .drain_motion_callbacks(EntityId(1), token(1), 0)
            .is_err()
    );
    assert!(world.take_motion_event().is_none());
    world.tick().unwrap();
    world
        .begin_motion(EntityId(1), token(2), source_program(true))
        .unwrap();
    assert!(
        world
            .drain_motion_callbacks(EntityId(1), token(1), 0)
            .is_err()
    );
    let initial = world.take_motion_event().unwrap();
    assert_eq!(initial.token, token(1));
    let before = world.body(EntityId(1)).unwrap().accepted();
    assert_eq!(
        world
            .drain_motion_callbacks(EntityId(1), initial.token, initial.epoch)
            .unwrap(),
        1
    );
    assert!(
        world
            .drain_motion_callbacks(EntityId(1), initial.token, initial.epoch)
            .is_err()
    );
    let pose = S {
        style: 0x80000049,
        substate: 0x4000002f,
        speed: 1.0,
    };
    let repeat = Arc::new(
        (*source_program(true))
            .clone()
            .with_source_transition(T {
                before: pose,
                after: pose,
                continues_cycle: true,
            })
            .unwrap(),
    );
    // The global bound is256 callback continuations in this physics phase.
    for sequence in 3..=257 {
        let completed = world.take_motion_event().unwrap();
        assert_eq!(completed.token, token(sequence - 1));
        world
            .begin_motion(EntityId(1), token(sequence), repeat.clone())
            .unwrap();
        assert_eq!(
            world
                .drain_motion_callbacks(EntityId(1), completed.token, completed.epoch)
                .unwrap(),
            1
        );
        assert_eq!(world.body(EntityId(1)).unwrap().accepted(), before);
    }
    let completed = world.take_motion_event().unwrap();
    world.begin_motion(EntityId(1), token(258), repeat).unwrap();
    assert!(matches!(
        world.drain_motion_callbacks(EntityId(1), completed.token, completed.epoch),
        Err(bace_world::WorldError::MotionBackpressure)
    ));
    assert!(world.motion_busy(EntityId(1)));
    assert!(world.take_motion_event().is_none());
    world.tick().unwrap();
    let next = world.take_motion_event().unwrap();
    assert_eq!(next.token, token(258));
    assert_eq!(next.event, MotionExecutionEvent::Completed);
    assert!(
        world
            .drain_motion_callbacks(EntityId(1), completed.token, completed.epoch)
            .is_err()
    );
}

#[path = "motions/style_action.rs"]
mod style_action;
