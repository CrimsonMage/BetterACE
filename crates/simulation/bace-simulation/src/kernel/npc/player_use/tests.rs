use super::*;
use bace_entity::Actor;
use bace_geometry::Aabb;
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{AccountId, CellId};

#[test]
fn cancelled_approach_releases_control_and_finishes_use_without_activation() {
    let cell = CellId(1);
    let mut world = bace_world::World::default();
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-5.0, -5.0, -1.0), Vec3::new(5.0, 5.0, 5.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let actor = EntityId(1);
    let body = Body::spawn_oriented(
        world.scene(cell).unwrap(),
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 1.0,
            jump_impulse: 1.0,
        },
        0.0,
        1.0,
    )
    .unwrap();
    world
        .insert(Actor {
            id: actor,
            cell,
            body,
        })
        .unwrap();
    let mut kernel = Kernel::new(world, 8).unwrap();
    let control = kernel.world.next_server_control(actor).unwrap();
    kernel
        .world
        .begin_server_move(
            actor,
            0,
            control,
            MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
        )
        .unwrap();
    kernel.npc_player_uses.insert(
        actor,
        Approach {
            context: ActionContext {
                actor,
                account: AccountId(1),
                session: bace_gameplay_api::SessionId(1),
                sequence: 1,
            },
            source: EntityId(2),
            event: [1; 16],
            operation: 1,
            control: Some(control),
            started_tick: 0,
            cancelled: false,
        },
    );

    kernel.cancel_player_npc_approach(actor);
    assert!(kernel.world.body(actor).unwrap().server_move().is_none());
    assert_eq!(kernel.poll_player_npc_use(actor, EntityId(2)), Ok(false));
    assert!(kernel.npc_player_uses.is_empty());
}
