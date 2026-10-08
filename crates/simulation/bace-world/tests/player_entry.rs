use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::World;
#[test]
fn loading_body_settles_but_cannot_attack_or_be_attacked_until_entered() {
    let scene = SyntheticScene::new(
        0.,
        Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
        vec![],
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene.clone()).unwrap();
    for (id, x) in [(1, 0.), (2, 2.)] {
        world
            .insert(Actor {
                id: EntityId(id),
                cell: CellId(1),
                body: Body::spawn(
                    &scene,
                    Vec3::new(x, 0., 3.),
                    0.5,
                    Capabilities {
                        speed: 3.,
                        jump_impulse: 1.,
                    },
                )
                .unwrap(),
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 1,
                    melee_range: 3.,
                    attack_duration: 1.,
                    strike_offsets: vec![0.5],
                    player: id == 1,
                })
                .unwrap(),
            )
            .unwrap();
    }
    assert!(
        world.begin_player_entry(EntityId(2)).is_err(),
        "NPCs cannot acquire the loading-player exclusion"
    );
    world.begin_player_entry(EntityId(1)).unwrap();
    assert!(world.begin_player_entry(EntityId(1)).is_err());
    assert!(world.is_in_portal_transit(EntityId(1)));
    assert_eq!(
        world.attack_geometry(EntityId(1), EntityId(2), 3.).unwrap(),
        (false, false)
    );
    assert_eq!(
        world.attack_geometry(EntityId(2), EntityId(1), 3.).unwrap(),
        (false, false)
    );
    let before = world.body(EntityId(1)).unwrap().accepted().position().z;
    for _ in 0..30 {
        world.tick().unwrap();
    }
    assert!(
        world.body(EntityId(1)).unwrap().accepted().position().z < before,
        "loading cannot suspend authoritative gravity"
    );
    world.finish_player_entry(EntityId(1));
    assert!(!world.is_in_portal_transit(EntityId(1)));
    assert_eq!(
        world.attack_geometry(EntityId(1), EntityId(2), 3.).unwrap(),
        (true, true)
    );
    assert_eq!(
        world.attack_geometry(EntityId(2), EntityId(1), 3.).unwrap(),
        (true, true)
    );
}
