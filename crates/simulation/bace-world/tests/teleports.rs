use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::{World, WorldTeleport};
fn scene() -> SyntheticScene {
    SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        vec![],
    )
    .unwrap()
}
fn actor(id: u32, p: Vec3) -> Actor {
    Actor {
        id: EntityId(id),
        cell: CellId(1),
        body: Body::spawn(
            &scene(),
            p,
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
fn group_teleport_preflight_preserves_every_actor_and_allows_atomic_swap() {
    let mut world = World::default();
    world.register_scene(CellId(1), scene()).unwrap();
    world.insert(actor(1, Vec3::new(1.0, 0.0, 0.5))).unwrap();
    world.insert(actor(2, Vec3::new(3.0, 0.0, 0.5))).unwrap();
    let before = [
        world.actor_state(EntityId(1)).unwrap(),
        world.actor_state(EntityId(2)).unwrap(),
    ];
    let mut requests = [
        WorldTeleport {
            actor: EntityId(1),
            expected_epoch: 0,
            destination: CellId(1),
            position: Vec3::new(3.0, 0.0, 0.5),
            heading: 1.0,
        },
        WorldTeleport {
            actor: EntityId(2),
            expected_epoch: 0,
            destination: CellId(9),
            position: Vec3::new(1.0, 0.0, 0.5),
            heading: 2.0,
        },
    ];
    assert!(world.teleport_batch(&requests).is_err());
    assert_eq!(
        [
            world.actor_state(EntityId(1)).unwrap(),
            world.actor_state(EntityId(2)).unwrap()
        ],
        before
    );
    requests[1].destination = CellId(1);
    world.teleport_batch(&requests).unwrap();
    let a = world.actor_state(EntityId(1)).unwrap().1;
    let b = world.actor_state(EntityId(2)).unwrap().1;
    assert_eq!(a.position(), requests[0].position);
    assert_eq!(b.position(), requests[1].position);
    assert_eq!(a.heading_radians(), 1.0);
    assert_eq!(b.heading_radians(), 2.0);
    assert_eq!(a.epoch(), 1);
    assert!(world.teleport_batch(&requests).is_err());
}
#[test]
fn static_anchor_has_one_queryable_pose_and_never_blocks_a_projectile() {
    let mut world = World::default();
    world.register_scene(CellId(1), scene()).unwrap();
    assert!(
        world
            .insert_anchor(actor(9, Vec3::new(3.0, 5.0, 2.0)))
            .is_ok()
    );
    let before = world.actor_state(EntityId(9)).unwrap();
    world.tick().unwrap();
    assert_eq!(world.actor_state(EntityId(9)).unwrap(), before);
    assert!(world.body_mut(EntityId(9)).is_err());
    assert!(world.teleport(EntityId(9), CellId(1), Vec3::ZERO).is_err());
    assert!(
        world
            .sweep_projectile(
                CellId(1),
                Vec3::new(3.0, 4.0, 2.0),
                Vec3::new(3.0, 6.0, 2.0),
                0.1,
                EntityId(1)
            )
            .unwrap()
            .is_none()
    );
    assert!(world.remove_anchor(EntityId(9)).is_some());
    assert!(!world.contains_identity(EntityId(9)));
}
#[test]
fn portal_space_is_owner_token_fenced_and_allows_shared_group_destination() {
    let mut world = World::default();
    world.register_scene(CellId(1), scene()).unwrap();
    world.insert(actor(1, Vec3::new(1.0, 0.0, 0.5))).unwrap();
    world.insert(actor(2, Vec3::new(3.0, 0.0, 0.5))).unwrap();
    let requests = [
        WorldTeleport {
            actor: EntityId(1),
            expected_epoch: 0,
            destination: CellId(1),
            position: Vec3::new(0.0, 5.0, 0.5),
            heading: 0.0,
        },
        WorldTeleport {
            actor: EntityId(2),
            expected_epoch: 0,
            destination: CellId(1),
            position: Vec3::new(0.0, 5.0, 0.5),
            heading: 0.0,
        },
    ];
    assert!(world.validate_teleport_batch(&requests).is_err());
    world
        .begin_portal_transit(&[(EntityId(1), 0), (EntityId(2), 0)], 77)
        .unwrap();
    assert!(world.body_mut(EntityId(1)).is_err());
    world.teleport_batch(&requests).unwrap();
    assert!(world.finish_portal_transit(EntityId(1), 76).is_err());
    assert!(world.is_in_portal_transit(EntityId(1)));
    world.finish_portal_transit(EntityId(1), 77).unwrap();
    world.finish_portal_transit(EntityId(2), 77).unwrap();
    assert_eq!(
        world.actor_state(EntityId(1)).unwrap().1.position(),
        world.actor_state(EntityId(2)).unwrap().1.position()
    );
}
