use super::*;
use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::CellId;
fn world() -> World {
    let scene = SyntheticScene::new(
        0.,
        Aabb::new(Vec3::new(-5., -5., -1.), Vec3::new(5., 5., 5.)).unwrap(),
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
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 1,
                    melee_range: 1.,
                    attack_duration: 1.,
                    strike_offsets: vec![0.5],
                    player: id < 3,
                })
                .unwrap(),
            )
            .unwrap();
    }
    world
}
#[test]
fn pk_activity_matches_original_integer_deadline_and_clear_vectors() {
    for line in include_str!("../../tests/fixtures/gdle_pk_activity.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let values: Vec<f64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        let mut world = world();
        record_pair(&mut world, EntityId(1), EntityId(2), values[0]);
        for id in [EntityId(1), EntityId(2)] {
            let actor = world.combatant_mut(id).unwrap();
            assert_eq!(
                actor.pk_activity_active(values[2]),
                values[3] != 0.,
                "{line}"
            );
            actor.clear_pk_activity();
            assert_eq!(
                actor.pk_activity_active(values[2]),
                values[4] != 0.,
                "{line}"
            );
            assert_eq!(
                actor.revision(),
                0,
                "PK timing is ephemeral, not a health mutation"
            );
        }
    }
}
#[test]
fn invalid_time_or_nonplayer_pair_never_changes_retained_player_timer() {
    let mut world = world();
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
        record_pair(&mut world, EntityId(1), EntityId(2), now);
        assert!(!world.combatant(EntityId(1)).unwrap().pk_activity_active(0.));
    }
    record_pair(&mut world, EntityId(1), EntityId(3), 10.);
    assert!(
        !world
            .combatant(EntityId(1))
            .unwrap()
            .pk_activity_active(10.)
    );
    record_pair(&mut world, EntityId(1), EntityId(2), 10.25);
    record_pair(&mut world, EntityId(1), EntityId(3), 100.);
    assert!(
        world
            .combatant(EntityId(1))
            .unwrap()
            .pk_activity_active(29.999)
    );
    assert!(
        !world
            .combatant(EntityId(1))
            .unwrap()
            .pk_activity_active(30.)
    );
}
