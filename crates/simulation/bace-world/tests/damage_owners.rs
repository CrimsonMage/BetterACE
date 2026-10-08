use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, ProjectileBody, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::{OwnedProjectile, World};
#[test]
fn pet_credit_survives_retirement_until_delayed_projectile_finishes() {
    let scene = SyntheticScene::new(
        0.,
        Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
        vec![],
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene.clone()).unwrap();
    for id in 1..=3 {
        world
            .insert(Actor {
                id: EntityId(id),
                cell: CellId(1),
                body: Body::spawn(
                    &scene,
                    Vec3::new(id as f32, 0., 0.5),
                    0.5,
                    Capabilities {
                        speed: 1.,
                        jump_impulse: 1.,
                    },
                )
                .unwrap(),
            })
            .unwrap();
    }
    world
        .register_damage_owner(EntityId(2), EntityId(1))
        .unwrap();
    assert_eq!(world.damage_owner(EntityId(2)), EntityId(1));
    assert!(
        world
            .register_damage_owner(EntityId(2), EntityId(3))
            .is_err()
    );
    assert!(
        world
            .register_damage_owner(EntityId(1), EntityId(3))
            .is_err()
    );
    let projectile = OwnedProjectile {
        cell: CellId(1),
        source: EntityId(2),
        target: Some(EntityId(3)),
        body: ProjectileBody::new(Vec3::new(2., 0., 1.), Vec3::new(1., 0., 0.), 0.1, 0., 10.)
            .unwrap(),
    };
    assert!(world.insert_projectile(EntityId(4), projectile).is_ok());
    world.remove(EntityId(2)).unwrap();
    assert_eq!(world.damage_owner(EntityId(2)), EntityId(1));
    assert!(world.contains_identity(EntityId(2)));
    world.remove_projectile(EntityId(4)).unwrap();
    assert_eq!(world.damage_owner(EntityId(2)), EntityId(2));
    assert!(!world.contains_identity(EntityId(2)));
}
#[test]
fn owned_actor_insertion_preflights_credit_owner_and_returns_actor_on_failure() {
    let scene = SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        vec![],
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene.clone()).unwrap();
    let actor = Actor {
        id: EntityId(7),
        cell: CellId(1),
        body: Body::spawn(
            &scene,
            Vec3::new(0.0, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 1.0,
                jump_impulse: 1.0,
            },
        )
        .unwrap(),
    };
    let Err((_, actor)) = world.insert_damage_owned(actor, EntityId(99)) else {
        panic!("missing owner must reject")
    };
    assert_eq!(actor.id, EntityId(7));
    assert!(!world.contains_identity(EntityId(7)));
    assert_eq!(world.damage_owner(EntityId(7)), EntityId(7));
}
