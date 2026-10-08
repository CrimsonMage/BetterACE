use super::*;
#[test]
fn use_gate_keeps_signed_threshold_and_rejects_nonfinite_input() {
    use bace_geometry::{Aabb, Vec3};
    use bace_types::CellId;
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            bace_physics::SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for (id, x) in [(1, 0.), (2, 1.), (3, 0.99)] {
        let body = bace_physics::Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(x, 0., 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 1.,
                jump_impulse: 1.,
            },
        )
        .unwrap();
        world
            .insert(bace_entity::Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
    }
    assert!(within_use_radius(&world, EntityId(1), EntityId(2), 0.).unwrap());
    assert!(!within_use_radius(&world, EntityId(1), EntityId(2), -0.1).unwrap());
    assert!(within_use_radius(&world, EntityId(1), EntityId(3), -0.1).unwrap());
    assert!(!within_use_radius(&world, EntityId(1), EntityId(3), -2.).unwrap());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            within_use_radius(&world, EntityId(1), EntityId(2), value),
            Err(NpcFailure::InvalidInput)
        );
    }
}
