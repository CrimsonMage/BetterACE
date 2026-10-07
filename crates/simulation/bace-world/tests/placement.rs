use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, PhysicsError, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::{World, WorldError};

fn scene(floor: f32, obstacles: Vec<Aabb>) -> SyntheticScene {
    SyntheticScene::new(
        floor,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        obstacles,
    )
    .unwrap()
}

fn actor(id: u32, cell: u32, source: &SyntheticScene) -> Actor {
    Actor {
        id: EntityId(id),
        cell: CellId(cell),
        body: Body::spawn(
            source,
            Vec3::new(0.0, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 1.0,
                jump_impulse: 1.0,
            },
        )
        .unwrap(),
    }
}

#[test]
fn insert_checks_actual_destination_geometry_before_taking_ownership() {
    let source = scene(0.0, vec![]);
    // Center is outside the wall, but the body's radius overlaps it.
    let wall = Aabb::new(Vec3::new(0.25, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0)).unwrap();
    let mut world = World::default();
    world
        .register_scene(CellId(1), scene(0.0, vec![wall]))
        .unwrap();
    assert!(matches!(
        world.insert(actor(7, 1, &source)),
        Err(WorldError::Physics(PhysicsError::InvalidState))
    ));
    assert_eq!(world.states().count(), 0);
    assert!(matches!(
        world.body(EntityId(7)),
        Err(WorldError::MissingActor)
    ));
    world.register_scene(CellId(2), source.clone()).unwrap();
    world.insert(actor(7, 2, &source)).unwrap();
    assert_eq!(world.states().count(), 1);
    let before = world.body(EntityId(7)).unwrap().accepted();
    assert!(matches!(
        world.insert(actor(7, 2, &source)),
        Err(WorldError::DuplicateActor)
    ));
    assert_eq!(world.body(EntityId(7)).unwrap().accepted(), before);
}

#[test]
fn missing_geometry_and_wrong_floor_are_rejected_without_partial_insert() {
    let source = scene(0.0, vec![]);
    let mut world = World::default();
    assert!(matches!(
        world.insert(actor(1, 1, &source)),
        Err(WorldError::MissingGeometry)
    ));
    world.register_scene(CellId(1), scene(2.0, vec![])).unwrap();
    assert!(matches!(
        world.insert(actor(1, 1, &source)),
        Err(WorldError::Physics(PhysicsError::InvalidState))
    ));
    assert_eq!(world.states().count(), 0);
}

#[test]
fn rejected_teleport_preserves_cell_pose_velocity_and_epoch() {
    let source = scene(0.0, vec![]);
    let mut world = World::default();
    world.register_scene(CellId(1), source.clone()).unwrap();
    world.register_scene(CellId(2), scene(2.0, vec![])).unwrap();
    world.insert(actor(1, 1, &source)).unwrap();
    let before: Vec<_> = world.states().collect();
    assert!(
        world
            .teleport(EntityId(1), CellId(2), Vec3::new(0.0, 0.0, 0.5))
            .is_err()
    );
    assert_eq!(world.states().collect::<Vec<_>>(), before);
    world
        .teleport(EntityId(1), CellId(2), Vec3::new(0.0, 0.0, 2.5))
        .unwrap();
    let (_, cell, state) = world.states().next().unwrap();
    assert_eq!(cell, CellId(2));
    assert_eq!(state.epoch(), before[0].2.epoch() + 1);
}
