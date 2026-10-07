use bace_geometry::{Aabb, Vec3};
use bace_motion::{Capabilities, MotionIntent};
use bace_physics::{Body, PhysicsError, STEP_SECONDS, SyntheticScene};

fn fixture() -> (SyntheticScene, Body) {
    let bounds = Aabb::new(
        Vec3::new(-100.0, -100.0, -1.0),
        Vec3::new(100.0, 100.0, 100.0),
    )
    .unwrap();
    let wall = Aabb::new(Vec3::new(1.0, -10.0, 0.0), Vec3::new(1.05, 10.0, 10.0)).unwrap();
    let scene = SyntheticScene::new(0.0, bounds, vec![wall]).unwrap();
    let body = Body::spawn(
        &scene,
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 50.0,
            jump_impulse: 5.0,
        },
    )
    .unwrap();
    (scene, body)
}

#[test]
fn sweep_stops_high_speed_motion_before_thin_wall() {
    let (scene, mut body) = fixture();
    body.submit_intent(
        0,
        1,
        MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
    )
    .unwrap();
    body.step(&scene);
    const {
        assert!(50.0 * STEP_SECONDS > 1.05);
    }
    assert!(body.accepted().position().x <= 0.5);
}

#[test]
fn reported_pose_never_sets_authority_and_teleport_invalidates_epoch() {
    let (scene, mut body) = fixture();
    let before = body.accepted();
    assert_eq!(
        body.observe(0, Vec3::new(90.0, 90.0, 90.0)).unwrap(),
        before
    );
    assert_eq!(body.accepted(), before);
    assert!(body.observe(0, Vec3::new(f32::NAN, 0.0, 0.0)).is_err());
    body.server_teleport(&scene, Vec3::new(-10.0, 0.0, 0.5))
        .unwrap();
    assert!(matches!(
        body.observe(0, Vec3::ZERO),
        Err(PhysicsError::StaleEpoch)
    ));
    assert_eq!(body.accepted().epoch(), 1);
}

#[test]
fn jump_uses_capability_and_cannot_repeat_in_midair() {
    let (scene, mut body) = fixture();
    let jump = MotionIntent::new(Vec3::ZERO, true).unwrap();
    body.submit_intent(0, u32::MAX, jump).unwrap();
    body.step(&scene);
    assert!(body.accepted().velocity().z <= 5.0);
    assert!(!body.accepted().grounded());
    let first = body.accepted().velocity().z;
    body.submit_intent(0, 0, jump).unwrap(); // Legal wrapping advance.
    body.step(&scene);
    assert!(body.accepted().velocity().z < first);
    assert!(matches!(
        body.submit_intent(0, 0, jump),
        Err(PhysicsError::StaleSequence)
    ));
}

#[test]
fn invalid_inputs_do_not_mutate_state() {
    let (scene, mut body) = fixture();
    let before = body.accepted();
    assert!(MotionIntent::new(Vec3::new(f32::INFINITY, 0.0, 0.0), true).is_err());
    assert!(
        body.server_teleport(&scene, Vec3::new(1.02, 0.0, 0.5))
            .is_err()
    );
    assert_eq!(body.accepted(), before);
    assert!(
        Body::spawn(
            &scene,
            Vec3::ZERO,
            0.5,
            Capabilities {
                speed: 1.0,
                jump_impulse: 1.0
            }
        )
        .is_err()
    );
}

#[test]
fn tangent_body_can_leave_or_move_along_wall_but_cannot_enter_it() {
    for (direction, expected_x, expected_y) in [
        (Vec3::new(-1.0, 0.0, 0.0), -1.0, 0.0),
        (Vec3::new(0.0, 1.0, 0.0), 0.0, 1.0),
        (Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0),
    ] {
        let (scene, mut body) = fixture();
        body.server_teleport(&scene, Vec3::new(0.5, 0.0, 0.5))
            .unwrap();
        body.submit_intent(1, 1, MotionIntent::new(direction, false).unwrap())
            .unwrap();
        body.step(&scene);
        let position = body.accepted().position();
        assert_eq!(position.x, 0.5 + expected_x * 50.0 * STEP_SECONDS);
        assert_eq!(position.y, expected_y * 50.0 * STEP_SECONDS);
        assert_eq!(position.z, 0.5);
        assert!(body.accepted().grounded());
    }
}
