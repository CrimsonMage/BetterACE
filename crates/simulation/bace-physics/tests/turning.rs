use bace_geometry::{Aabb, Vec3};
use bace_motion::{Capabilities, MotionIntent, TurnControl, TurnIntent};
use bace_physics::{Body, SyntheticScene};
#[test]
fn legal_turn_is_rate_bounded_and_manual_movement_cancels_server_turn() {
    let scene = SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-20.0, -20.0, -1.0), Vec3::new(20.0, 20.0, 20.0)).unwrap(),
        vec![],
    )
    .unwrap();
    let mut body = Body::spawn_oriented(
        &scene,
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 5.0,
            jump_impulse: 5.0,
        },
        0.0,
        3.0,
    )
    .unwrap();
    let control = TurnControl {
        owner: 1,
        sequence: 1,
    };
    body.begin_server_turn(0, control, TurnIntent::new(1.0).unwrap())
        .unwrap();
    body.step(&scene);
    assert!((body.accepted().heading_radians() - 0.1).abs() < 1e-6);
    body.submit_intent(
        0,
        1,
        MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
    )
    .unwrap();
    assert_eq!(body.server_turn(), None);
    assert!(
        body.continue_server_turn(0, control, TurnIntent::new(1.0).unwrap())
            .is_err()
    );
    body.submit_turn_intent(0, 1, TurnIntent::new(-1.0).unwrap())
        .unwrap();
    assert!(body.manual_turning());
    let before = body.accepted();
    assert!(TurnIntent::new(f32::NAN).is_err());
    assert!(TurnIntent::new(2.0).is_err());
    assert_eq!(body.accepted(), before);
    body.server_teleport(&scene, Vec3::new(2.0, 0.0, 0.5))
        .unwrap();
    assert!(
        body.submit_turn_intent(0, 2, TurnIntent::new(1.0).unwrap())
            .is_err()
    );
}
