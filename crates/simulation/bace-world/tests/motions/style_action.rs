use super::*;
use bace_motion::{SourceMotionState, SourceMotionTransition};

fn sequence() -> (Arc<PreparedMotionChain>, Arc<PreparedMotionChain>) {
    let ready = SourceMotionState {
        style: 0x8000003c,
        substate: 0x41000003,
        speed: 1.0,
    };
    let noncombat = SourceMotionState {
        style: 0x8000003d,
        ..ready
    };
    let make = |motion, before, after, id| {
        let clip = |animation, frames| ExecutionClip {
            animation,
            frame_count: frames,
            low: 0,
            high: -1,
            framerate: 30.0,
            frames: vec![
                RootFrame {
                    translation: Vec3::ZERO,
                    heading: 0.0
                };
                frames as usize
            ]
            .into(),
            hooks: Vec::new().into(),
        };
        Arc::new(
            PreparedMotionChain::prepare(
                vec![clip(id, 3), clip(99, 2)],
                1,
                1,
                MotionPhysics {
                    velocity: Vec3::ZERO,
                    omega: Vec3::ZERO,
                },
                motion,
                1.0,
            )
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before,
                after,
                continues_cycle: false,
            })
            .unwrap(),
        )
    };
    (
        make(noncombat.style, ready, noncombat, 21),
        make(0x1300007e, noncombat, noncombat, 22),
    )
}

#[test]
fn style_and_action_are_admitted_together_with_ordered_callbacks() {
    let mut world = world();
    let (style, action) = sequence();
    world
        .begin_style_action(EntityId(1), token(1), style, token(2), action.clone())
        .unwrap();
    assert_eq!(
        world.source_motion_state(EntityId(1)),
        action.source_transition().map(|s| s.after)
    );
    assert_eq!(world.source_motion_token(EntityId(1)), Some(token(2)));
    assert!(world.has_pending_action_motion(EntityId(1)));
    let mut done = Vec::new();
    for _ in 0..12 {
        world.tick().unwrap();
        while let Some(event) = world.take_motion_event() {
            if event.event == MotionExecutionEvent::Completed {
                done.push(event.token);
            }
        }
    }
    assert_eq!(done, vec![token(1), token(2)]);
}

#[test]
fn invalid_second_command_and_replay_preserve_the_live_sequence() {
    let mut world = world();
    let (style, action) = sequence();
    let state = world.actor_state(EntityId(1)).unwrap().1;
    let mut wrong = action.source_transition().unwrap();
    wrong.before.style = 0x8000003c;
    let invalid = Arc::new((*action).clone().with_source_transition(wrong).unwrap());
    assert!(
        world
            .begin_style_action(EntityId(1), token(1), style.clone(), token(2), invalid)
            .is_err()
    );
    assert_eq!(world.source_motion_token(EntityId(1)), None);
    assert_eq!(world.actor_state(EntityId(1)).unwrap().1, state);
    world
        .begin_style_action(
            EntityId(1),
            token(1),
            style.clone(),
            token(2),
            action.clone(),
        )
        .unwrap();
    assert!(
        world
            .begin_style_action(EntityId(1), token(3), style, token(4), action)
            .is_err()
    );
    assert_eq!(world.source_motion_token(EntityId(1)), Some(token(2)));
    assert!(world.has_pending_action_motion(EntityId(1)));
}

#[test]
fn mixed_owners_and_reversed_tokens_do_not_admit_either_command() {
    for second in [
        MotionToken {
            owner: 2,
            ..token(2)
        },
        token(1),
    ] {
        let mut world = world();
        let (style, action) = sequence();
        assert!(
            world
                .begin_style_action(EntityId(1), token(1), style, second, action)
                .is_err()
        );
        assert_eq!(world.source_motion_token(EntityId(1)), None);
        assert!(!world.has_pending_action_motion(EntityId(1)));
    }
}
