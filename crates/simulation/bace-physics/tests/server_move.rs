use bace_geometry::{Aabb, Vec3};
use bace_motion::{Capabilities, MotionIntent, TurnControl, TurnIntent};
use bace_physics::{Body, SyntheticScene};

fn scene() -> SyntheticScene {
    SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-20., -20., -1.), Vec3::new(20., 20., 20.)).unwrap(),
        vec![Aabb::new(Vec3::new(2., -5., 0.), Vec3::new(3., 5., 5.)).unwrap()],
    )
    .unwrap()
}
#[test]
fn approach_sweeps_and_does_not_consume_client_sequence() {
    let scene = scene();
    let mut body = Body::spawn(
        &scene,
        Vec3::new(0., 0., 0.5),
        0.5,
        Capabilities {
            speed: 5.,
            jump_impulse: 5.,
        },
    )
    .unwrap();
    let control = TurnControl {
        owner: 1,
        sequence: 1,
    };
    let intent = MotionIntent::new(Vec3::new(1000., 0., 0.), false).unwrap();
    body.begin_server_move(0, control, intent).unwrap();
    body.step(&scene);
    assert!(body.accepted().position().x <= 5. / 30. + 1e-5);
    for _ in 0..60 {
        body.step(&scene);
    }
    assert!(body.accepted().position().x <= 1.501);
    body.submit_intent(0, 1, MotionIntent::new(Vec3::ZERO, false).unwrap())
        .unwrap();
    assert_eq!(body.server_move(), None);
    assert!(body.continue_server_move(0, control, intent).is_err());
    assert!(body.begin_server_move(0, control, intent).is_err());
}
#[test]
fn stale_stop_turn_and_teleport_cannot_reactivate_approach() {
    let scene = scene();
    let mut body = Body::spawn(
        &scene,
        Vec3::new(0., 0., 0.5),
        0.5,
        Capabilities {
            speed: 5.,
            jump_impulse: 5.,
        },
    )
    .unwrap();
    let one = TurnControl {
        owner: 1,
        sequence: 1,
    };
    let two = TurnControl {
        owner: 1,
        sequence: 2,
    };
    let intent = MotionIntent::new(Vec3::new(1., 0., 0.), false).unwrap();
    assert!(
        body.begin_server_move(0, one, MotionIntent::new(Vec3::ZERO, true).unwrap())
            .is_err()
    );
    body.begin_server_move(0, one, intent).unwrap();
    body.begin_server_move(0, two, intent).unwrap();
    body.finish_server_move(one);
    assert_eq!(body.server_move(), Some(two));
    body.submit_turn_intent(0, 1, TurnIntent::new(1.).unwrap())
        .unwrap();
    assert_eq!(body.server_move(), None);
    body.server_teleport(&scene, Vec3::new(-1., 0., 0.5))
        .unwrap();
    assert!(body.continue_server_move(0, two, intent).is_err());
    assert!(body.begin_server_move(1, two, intent).is_err());
}
#[test]
fn allocator_is_shared_and_finish_does_not_resume_stale_manual_motion() {
    let scene = scene();
    let mut body = Body::spawn(
        &scene,
        Vec3::new(0., 0., 0.5),
        0.5,
        Capabilities {
            speed: 5.,
            jump_impulse: 5.,
        },
    )
    .unwrap();
    let moving = MotionIntent::new(Vec3::new(1., 0., 0.), false).unwrap();
    body.submit_intent(0, 1, moving).unwrap();
    let first = body.next_server_control().unwrap();
    let second = body.next_server_control().unwrap();
    assert!(second > first);
    body.begin_server_move(0, first, moving).unwrap();
    body.begin_server_turn(0, second, TurnIntent::new(1.0).unwrap())
        .unwrap();
    body.finish_server_move(first);
    body.finish_server_turn(second);
    let before = body.accepted().position();
    body.step(&scene);
    assert_eq!(body.accepted().position(), before);
    assert!(body.next_server_control().unwrap() > second);
}

#[test]
fn authored_speed_is_bounded_before_control_or_capability_mutation() {
    let scene = scene();
    let mut body = Body::spawn(
        &scene,
        Vec3::new(0., 0., 0.5),
        0.5,
        Capabilities {
            speed: 5.,
            jump_impulse: 5.,
        },
    )
    .unwrap();
    let control = body.next_server_control().unwrap();
    let intent = MotionIntent::new(Vec3::new(0., 1., 0.), false).unwrap();
    for speed in [f32::NAN, f32::INFINITY, 0., -1., 10.01, 21.] {
        assert!(
            body.begin_server_move_scaled(0, control, intent, speed)
                .is_err()
        );
        assert_eq!(body.server_move(), None);
    }
    body.begin_server_move_scaled(0, control, intent, 2.)
        .unwrap();
    body.step(&scene);
    assert!((body.accepted().position().y - 10. / 30.).abs() < 1e-5);
    assert!(
        body.update_capabilities(Capabilities {
            speed: 30.,
            jump_impulse: 5.
        })
        .is_err()
    );
    assert!(
        body.continue_server_move_scaled(0, control, intent, f32::NAN)
            .is_err()
    );
    body.step(&scene);
    assert!((body.accepted().position().y - 20. / 30.).abs() < 1e-5);
    body.finish_server_move(control);
    let control = body.next_server_control().unwrap();
    body.begin_server_move(0, control, intent).unwrap();
    body.step(&scene);
    assert!((body.accepted().position().y - 25. / 30.).abs() < 1e-5);
}
