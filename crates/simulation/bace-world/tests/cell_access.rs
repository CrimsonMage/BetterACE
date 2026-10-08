use bace_geometry::Vec3;
use bace_physics::{CollisionFace, CollisionPlane, GdlePolygon, GeometryCell, GeometryRegion};
use bace_types::{CellId, EntityId};
use bace_world::World;
#[test]
fn stale_house_grants_cannot_open_a_restricted_cell() {
    let cell = GeometryCell {
        id: 1,
        terrain: false,
        restriction: Some(42),
        solids: vec![],
        static_primitives: vec![],
        boundary: vec![CollisionPlane {
            normal: Vec3::new(0., 0., 1.),
            distance: 0.,
        }],
        faces: vec![CollisionFace {
            polygon: GdlePolygon::prepare(vec![
                Vec3::new(0., 0., 0.),
                Vec3::new(10., 0., 0.),
                Vec3::new(0., 10., 0.),
            ])
            .unwrap(),
            two_sided: true,
            object: None,
        }],
        portals: vec![],
    };
    let mut world = World::default();
    world
        .install_geometry(std::sync::Arc::new(
            GeometryRegion::prepare(vec![cell]).unwrap(),
        ))
        .unwrap();
    assert!(
        world
            .validate_player_cell_entry(EntityId(7), CellId(1))
            .is_err()
    );
    assert!(world.set_cell_access(EntityId(7), 42, 1, true).is_err());
    world.set_restriction_generation(42, 1).unwrap();
    world.set_cell_access(EntityId(7), 42, 1, true).unwrap();
    world
        .validate_player_cell_entry(EntityId(7), CellId(1))
        .unwrap();
    world.set_restriction_generation(42, 2).unwrap();
    assert!(!world.has_cell_access(EntityId(7), 42));
    assert!(
        world
            .validate_player_cell_entry(EntityId(7), CellId(1))
            .is_err()
    );
    assert!(world.set_cell_access(EntityId(7), 42, 1, true).is_err());
    assert!(world.set_restriction_generation(42, 1).is_err());
    world.set_cell_access(EntityId(7), 42, 2, true).unwrap();
    world.set_cell_access(EntityId(7), 42, 2, false).unwrap();
    assert!(!world.has_cell_access(EntityId(7), 42));
}
