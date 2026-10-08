use bace_geometry::{Aabb, Vec3};
use bace_physics::SyntheticScene;
#[test]
fn melee_segment_rejects_wall_and_invalid_endpoints() {
    let scene = SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        vec![Aabb::new(Vec3::new(0.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 3.0)).unwrap()],
    )
    .unwrap();
    assert!(!scene.segment_clear(Vec3::new(-1.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 1.0)));
    assert!(scene.segment_clear(Vec3::new(-1.0, 3.0, 1.0), Vec3::new(2.0, 3.0, 1.0)));
    assert!(!scene.segment_clear(Vec3::new(f32::NAN, 0.0, 1.0), Vec3::new(2.0, 3.0, 1.0)));
    assert!(!scene.segment_clear(Vec3::new(-1.0, 3.0, 1.0), Vec3::new(20.0, 3.0, 1.0)));
}
