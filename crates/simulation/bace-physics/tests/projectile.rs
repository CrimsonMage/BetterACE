use bace_geometry::Vec3;
use bace_physics::{
    CollisionShape, CollisionSphere, ProjectileBody, ProjectileHit, ProjectileStep,
};
#[test]
fn invalid_contact_and_quantum_retain_body_and_subfloat_lifetime_can_expire() {
    let mut body =
        ProjectileBody::new(Vec3::ZERO, Vec3::new(20.0, 0.0, 10.0), 0.1, -9.8, 10.0).unwrap();
    let before = body.clone();
    assert!(
        body.step(
            0.1,
            Some(ProjectileHit {
                fraction: f32::NAN,
                target: None
            })
        )
        .is_err()
    );
    assert_eq!(body, before);
    assert!(body.step(0.20001, None).is_err());
    assert_eq!(body, before);
    let mut tiny =
        ProjectileBody::new(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), 0.1, 0.0, 1e-300).unwrap();
    assert_eq!(
        tiny.step(1.0 / 30.0, None).unwrap(),
        ProjectileStep::Expired
    );
    assert_eq!(
        tiny.step(1.0 / 30.0, None).unwrap(),
        ProjectileStep::Finished
    );
}
#[test]
fn nominal_setup_dimensions_are_not_sphere_extents_or_synthetic_defaults() {
    let shape = CollisionShape::prepare(
        vec![CollisionSphere {
            center: Vec3::ZERO,
            radius: 0.5,
        }],
        0.0,
        0.0,
    )
    .unwrap();
    assert_eq!(shape.nominal_radius(), None);
    assert_eq!(shape.nominal_height(), None);
    let shape = shape.with_nominal_dimensions(0.8, 2.0).unwrap();
    assert_eq!(shape.nominal_radius(), Some(0.8));
    assert_eq!(shape.nominal_height(), Some(2.0));
    assert_ne!(shape.nominal_height(), Some(shape.height()));
}
