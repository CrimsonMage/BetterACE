use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::World;
fn profile(player: bool) -> Combatant {
    Combatant::new(CombatantProfile {
        maximum_health: 100,
        melee_damage: 1,
        melee_range: 4.,
        attack_duration: 1.,
        strike_offsets: vec![0.5],
        player,
    })
    .unwrap()
}
#[test]
fn fresh_npc_hold_settles_but_remains_untargetable_until_exact_release() {
    let scene = SyntheticScene::new(
        0.,
        Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
        vec![],
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene.clone()).unwrap();
    world
        .validate_npc_admissions([EntityId(2), EntityId(3)].into_iter())
        .unwrap();
    for id in 1..=3 {
        world
            .insert(Actor {
                id: EntityId(id),
                cell: CellId(1),
                body: Body::spawn(
                    &scene,
                    Vec3::new(id as f32, 0., 3.),
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
            .register_combatant(EntityId(id), profile(id == 1))
            .unwrap();
    }
    assert!(
        world.begin_npc_admission(EntityId(1)).is_err(),
        "player entry is a separate owner"
    );
    let hold = world.begin_npc_admission(EntityId(2)).unwrap();
    let other = world.begin_npc_admission(EntityId(3)).unwrap();
    assert!(world.finish_npc_admission(EntityId(2), other).is_err());
    assert_eq!(
        world.attack_geometry(EntityId(1), EntityId(2), 4.).unwrap(),
        (false, false)
    );
    assert_eq!(
        world.attack_geometry(EntityId(2), EntityId(1), 4.).unwrap(),
        (false, false)
    );
    let mut visible = Vec::new();
    world
        .visibility_candidates(EntityId(1), &mut visible, 16)
        .unwrap();
    assert!(visible.iter().all(|v| v.entity != EntityId(2)));
    for _ in 0..30 {
        world.tick().unwrap();
    }
    assert!(world.body(EntityId(2)).unwrap().accepted().position().z < 3.);
    world.finish_npc_admission(EntityId(2), hold).unwrap();
    assert_eq!(
        world.attack_geometry(EntityId(1), EntityId(2), 4.).unwrap(),
        (true, true)
    );
    world
        .visibility_candidates(EntityId(1), &mut visible, 16)
        .unwrap();
    assert!(visible.iter().any(|v| v.entity == EntityId(2)));
    let actor = world.remove(EntityId(2)).unwrap();
    world.insert(actor).unwrap();
    world
        .register_combatant(EntityId(2), profile(false))
        .unwrap();
    let fresh = world.begin_npc_admission(EntityId(2)).unwrap();
    assert_ne!(fresh, hold);
    assert!(world.finish_npc_admission(EntityId(2), hold).is_err());
    assert_eq!(world.npc_admission_hold(EntityId(2)), Some(fresh));
    world.finish_npc_admission(EntityId(2), fresh).unwrap();
    world.finish_npc_admission(EntityId(3), other).unwrap();
}
