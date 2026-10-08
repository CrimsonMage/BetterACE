//! Explicit synthetic prepared cells exercise owner/frame/filter boundaries.
use bace_entity::Actor;
use bace_geometry::Vec3;
use bace_motion::Capabilities;
use bace_physics::{
    CellPortalGeometry, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon, GeometryCell,
    GeometryRegion, GeometrySpawn, ProjectileBody, ProjectileStep,
};
use bace_types::{CellId, EntityId};
use bace_world::{OwnedProjectile, World};
use std::sync::Arc;
fn cell(id: u32, left: f32, right: f32) -> GeometryCell {
    GeometryCell {
        id,
        terrain: false,
        restriction: None,
        solids: vec![],
        static_primitives: vec![],
        faces: vec![],
        portals: vec![],
        boundary: vec![
            CollisionPlane {
                normal: Vec3::new(1.0, 0.0, 0.0),
                distance: -left,
            },
            CollisionPlane {
                normal: Vec3::new(-1.0, 0.0, 0.0),
                distance: right,
            },
        ],
    }
}
fn portal(x: f32, destination: u32, translation: Vec3) -> CellPortalGeometry {
    CellPortalGeometry {
        destination,
        translation,
        polygon: GdlePolygon::prepare(vec![
            Vec3::new(x, -5.0, 0.0),
            Vec3::new(x, 5.0, 0.0),
            Vec3::new(x, 5.0, 8.0),
            Vec3::new(x, -5.0, 8.0),
        ])
        .unwrap(),
    }
}
fn actor(world: &mut World, id: u32, cell: u32, position: Vec3) {
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::ZERO,
                radius: 0.3,
            }],
            0.0,
            0.0,
        )
        .unwrap(),
    );
    let body = world
        .prepare_geometry_body(GeometrySpawn {
            cell,
            position,
            shape,
            capabilities: Capabilities {
                speed: 3.0,
                jump_impulse: 1.0,
            },
            heading: 0.0,
            maximum_turn_rate: 1.0,
        })
        .unwrap();
    world
        .insert(Actor {
            id: EntityId(id),
            cell: CellId(cell),
            body,
        })
        .unwrap();
}
fn shot(world: &mut World, id: u32, cell: u32, from: Vec3, velocity: Vec3) {
    let body = ProjectileBody::new(from, velocity, 0.1, 0.0, 10.0).unwrap();
    assert!(
        world
            .insert_projectile(
                EntityId(id),
                OwnedProjectile {
                    cell: CellId(cell),
                    source: EntityId(99),
                    target: Some(EntityId(2)),
                    body
                }
            )
            .is_ok()
    );
}
#[test]
fn target_filter_does_not_stop_at_ignored_actor_and_impact_adopts_destination_frame() {
    let mut first = cell(1, 0.0, 5.0);
    let mut second = cell(2, 0.0, 5.0);
    first
        .portals
        .push(portal(5.0, 2, Vec3::new(-5.0, 0.0, 0.0)));
    second
        .portals
        .push(portal(0.0, 1, Vec3::new(5.0, 0.0, 0.0)));
    let mut world = World::default();
    world
        .install_geometry(Arc::new(
            GeometryRegion::prepare(vec![first, second]).unwrap(),
        ))
        .unwrap();
    actor(&mut world, 1, 1, Vec3::new(4.7, 0.0, 1.0));
    actor(&mut world, 2, 2, Vec3::new(1.0, 0.0, 1.0));
    shot(
        &mut world,
        7,
        1,
        Vec3::new(4.0, 0.0, 1.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    assert_eq!(
        world
            .step_projectile_filtered(EntityId(7), 0.1, |_, id| id == EntityId(2))
            .unwrap(),
        ProjectileStep::Impact { target: Some(2) }
    );
    let p = world.projectile(EntityId(7)).unwrap();
    assert_eq!(p.cell, CellId(2));
    assert!((p.body.position().x - 0.6).abs() < 0.001);
    assert!(
        (world
            .projectile_distance_from(EntityId(7), CellId(1), Vec3::new(3.0, 0.0, 1.0))
            .unwrap()
            - 2.6)
            .abs()
            < 0.001
    );
    assert!(
        world
            .projectile_distance_from(EntityId(7), CellId(2), Vec3::ZERO)
            .is_err()
    );
    assert_eq!(
        world.step_projectile(EntityId(7), 0.1).unwrap(),
        ProjectileStep::Finished
    );
    shot(
        &mut world,
        8,
        1,
        Vec3::new(4.0, 0.0, 1.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    assert_eq!(
        world.step_projectile(EntityId(8), 0.1).unwrap(),
        ProjectileStep::Impact { target: Some(1) }
    );
}
#[test]
fn failed_trace_retains_flight_and_actor_projection_requires_real_landblock_metric() {
    let first = cell(0x01010001, 0.0, 5.0);
    let second = cell(0x02010001, 0.0, 5.0);
    let cells = vec![first, second];
    let mut world = World::default();
    world
        .install_geometry(Arc::new(GeometryRegion::prepare(cells.clone()).unwrap()))
        .unwrap();
    actor(&mut world, 2, 0x02010001, Vec3::new(1.0, 0.0, 1.0));
    assert!(
        world
            .actor_in_frame(EntityId(2), CellId(0x01010001))
            .is_err()
    );
    world
        .install_geometry(Arc::new(
            GeometryRegion::prepare(cells)
                .unwrap()
                .with_landblock_metric(24.0, 8)
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        world
            .actor_in_frame(EntityId(2), CellId(0x01010001))
            .unwrap(),
        (Vec3::new(193.0, 0.0, 1.0), Vec3::ZERO)
    );
    shot(
        &mut world,
        7,
        0x01010001,
        Vec3::new(4.0, 0.0, 1.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    let before = world.projectile(EntityId(7)).unwrap().body.clone();
    assert!(world.step_projectile(EntityId(7), 0.1).is_err());
    let after = world.projectile(EntityId(7)).unwrap();
    assert_eq!(after.body, before);
    assert_eq!(after.cell, CellId(0x01010001));
}
