use bace_entity::{Actor, Combatant, CombatantProfile, EntityVital, VitalMutation, VitalPool};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::{VitalReservationDomain, VitalReservationToken, World};
fn world() -> World {
    let scene = SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        vec![],
    )
    .unwrap();
    let body = Body::spawn(
        &scene,
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 3.0,
            jump_impulse: 1.0,
        },
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene).unwrap();
    world
        .insert(Actor {
            id: EntityId(1),
            cell: CellId(1),
            body,
        })
        .unwrap();
    let pool = Some(VitalPool {
        current: 100,
        maximum: 100,
    });
    world
        .register_combatant(
            EntityId(1),
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 1.0,
                attack_duration: 1.0,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap()
            .with_resources(pool, pool)
            .unwrap(),
        )
        .unwrap();
    world
}
#[test]
fn reservations_hold_mana_across_health_changes_and_exact_retry_without_partial_batch() {
    let mut world = world();
    let token = VitalReservationToken {
        domain: VitalReservationDomain::SpellComponents,
        operation: 1,
    };
    let other = VitalReservationToken {
        domain: VitalReservationDomain::Portal,
        operation: 1,
    };
    let mana = (EntityId(1), EntityVital::Mana);
    world.reserve_vitals(&[mana], token).unwrap();
    world.reserve_vitals(&[mana], token).unwrap();
    assert!(world.reserve_vitals(&[mana], other).is_err());
    assert!(
        world
            .reserve_vitals(&[mana, (EntityId(1), EntityVital::Stamina)], token)
            .is_err()
    );
    assert!(!world.vital_reserved(EntityId(1), EntityVital::Stamina));
    let health = VitalMutation {
        actor: EntityId(1),
        vital: EntityVital::Health,
        before: 100,
        after: 80,
    };
    let spend = VitalMutation {
        actor: EntityId(1),
        vital: EntityVital::Mana,
        before: 100,
        after: 70,
    };
    assert!(world.apply_vital_batch(&[health, spend], None).is_err());
    assert_eq!(
        world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        100
    );
    world.apply_vital_batch(&[health], None).unwrap();
    assert!(
        world
            .apply_vital_batch_reserved(&[spend], None, other)
            .is_err()
    );
    world
        .apply_vital_batch_reserved(&[spend], None, token)
        .unwrap();
    assert!(world.remove(EntityId(1)).is_none());
    assert!(world.has_vital_reservations());
    world.release_vitals(other);
    assert!(world.has_vital_reservations());
    world.release_vitals(token);
    assert!(!world.has_vital_reservations());
    assert!(world.remove(EntityId(1)).is_some());
}
