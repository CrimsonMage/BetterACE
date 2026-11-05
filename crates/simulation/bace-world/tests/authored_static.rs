use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{
    Body, CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon,
    GeometryCell, GeometryRegion, GeometrySpawn,
};
use bace_types::{CellId, EntityId};
use bace_world::{DoorCollider, World};
use std::sync::Arc;

const CELL: u32 = 0xe74e0014;

fn terrain() -> Arc<GeometryRegion> {
    Arc::new(
        GeometryRegion::prepare(vec![GeometryCell {
            id: CELL,
            terrain: true,
            restriction: None,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![
                CollisionPlane {
                    normal: Vec3::new(1., 0., 0.),
                    distance: 0.,
                },
                CollisionPlane {
                    normal: Vec3::new(-1., 0., 0.),
                    distance: 24.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., 1., 0.),
                    distance: 0.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., -1., 0.),
                    distance: 24.,
                },
            ],
            faces: vec![CollisionFace {
                polygon: GdlePolygon::prepare(vec![
                    Vec3::new(0., 0., 0.),
                    Vec3::new(24., 0., 0.),
                    Vec3::new(24., 24., 0.),
                    Vec3::new(0., 24., 0.),
                ])
                .unwrap(),
                two_sided: false,
                object: None,
            }],
            portals: vec![],
        }])
        .unwrap(),
    )
}

#[test]
fn authored_stationary_portal_survives_world_admission_tick_and_geometry_replacement() {
    let region = terrain();
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::ZERO,
                radius: 1.0,
            }],
            0.0,
            0.0,
        )
        .unwrap(),
    );
    let spawn = || GeometrySpawn {
        cell: CELL,
        position: Vec3::new(8., 7., 0.1),
        shape: shape.clone(),
        capabilities: Capabilities {
            speed: 0.,
            jump_impulse: 0.,
        },
        heading: 0.,
        maximum_turn_rate: 0.,
    };
    assert!(Body::spawn_geometry(&region, spawn()).is_err());
    let body = Body::spawn_authored_static_geometry(&region, spawn()).unwrap();
    let position = body.accepted().position();
    let actor = Actor {
        id: EntityId(7),
        cell: CellId(CELL),
        body,
    };
    let mut world = World::default();
    world
        .install_geometry_and_actors(region.clone(), vec![actor])
        .unwrap();
    world.tick().unwrap();
    world.install_geometry(terrain()).unwrap();
    assert_eq!(
        world.body(EntityId(7)).unwrap().accepted().position(),
        position
    );
}

#[test]
fn overlapping_authored_stationary_objects_keep_collision_against_mobile_actors() {
    let region = terrain();
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 1.),
                radius: 0.3,
            }],
            0.0,
            0.0,
        )
        .unwrap(),
    );
    let spawn = |x| GeometrySpawn {
        cell: CELL,
        position: Vec3::new(x, 7., 0.5),
        shape: shape.clone(),
        capabilities: Capabilities {
            speed: 0.,
            jump_impulse: 0.,
        },
        heading: 0.,
        maximum_turn_rate: 0.,
    };
    let mut world = World::default();
    world.install_geometry(region.clone()).unwrap();
    for (id, x) in [(1, 8.0), (2, 8.1)] {
        world
            .insert(Actor {
                id: EntityId(id),
                cell: CellId(CELL),
                body: Body::spawn_authored_stationary_geometry(&region, spawn(x)).unwrap(),
            })
            .unwrap();
    }
    let mobile = Actor {
        id: EntityId(3),
        cell: CellId(CELL),
        body: Body::spawn_geometry(&region, spawn(8.2)).unwrap(),
    };
    assert!(matches!(
        world.insert(mobile),
        Err(bace_world::WorldError::ActorOverlap(_))
    ));
    world.tick().unwrap();
    assert_eq!(
        world.body(EntityId(2)).unwrap().accepted().position(),
        Vec3::new(8.1, 7., 0.5)
    );
}

#[test]
fn authored_door_attaches_to_one_stationary_root_and_changes_solidity() {
    let region = terrain();
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 1.),
                radius: 0.3,
            }],
            0.,
            0.,
        )
        .unwrap(),
    );
    let position = Vec3::new(8., 7., 0.5);
    let body = Body::spawn_authored_stationary_geometry(
        &region,
        GeometrySpawn {
            cell: CELL,
            position,
            shape,
            capabilities: Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
            heading: 0.,
            maximum_turn_rate: 0.,
        },
    )
    .unwrap();
    let mut world = World::default();
    world
        .install_geometry_and_actors(
            region,
            vec![Actor {
                id: EntityId(10),
                cell: CellId(CELL),
                body,
            }],
        )
        .unwrap();
    let collider = DoorCollider {
        cell: CellId(CELL),
        position,
        bounds: Aabb::new(position, position + Vec3::new(0.4, 0.4, 2.)).unwrap(),
        solid: true,
    };
    let mut wrong = collider;
    wrong.position.x += 1.;
    assert!(world.register_door(EntityId(10), wrong).is_err());
    assert!(world.door(EntityId(10)).is_none());
    world.register_door(EntityId(10), collider).unwrap();
    assert!(world.register_door(EntityId(10), collider).is_err());
    world.set_door_solid(EntityId(10), false).unwrap();
    assert!(!world.door(EntityId(10)).unwrap().solid);
    assert_eq!(
        world.body(EntityId(10)).unwrap().accepted().position(),
        position
    );
    world.install_geometry(terrain()).unwrap();
    assert!(!world.door(EntityId(10)).unwrap().solid);
    world.set_door_solid(EntityId(10), true).unwrap();
    assert!(world.door(EntityId(10)).unwrap().solid);
}
