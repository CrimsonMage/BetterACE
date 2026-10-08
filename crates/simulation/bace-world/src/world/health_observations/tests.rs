use super::*;
use bace_entity::{Actor, Combatant, CombatantProfile, EntityVital, VitalMutation, VitalPool};
use bace_gameplay_api::{ActionContext, SessionId};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::AccountId;
use bace_types::{CellId, EntityId};
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

fn context() -> ActionContext {
    ActionContext {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
        sequence: 4,
    }
}
#[test]
fn accepted_intermediate_health_and_old_subscription_are_retained() {
    let mut w = world();
    w.set_health_subscription(context(), Some(EntityId(1)))
        .unwrap();
    w.damage_from(EntityId(1), EntityId(2), 10).unwrap();
    w.apply_vital_batch(
        &[VitalMutation {
            actor: EntityId(1),
            vital: EntityVital::Health,
            before: 90,
            after: 95,
        }],
        None,
    )
    .unwrap();
    w.set_health_subscription(context(), None).unwrap();
    assert!(w.remove(EntityId(1)).is_none());
    let a = w.take_health_observation().unwrap();
    let b = w.take_health_observation().unwrap();
    assert_eq!((a.current, b.current, a.maximum), (90, 95, 100));
    assert_eq!(a.context, context());
    assert!(w.remove(EntityId(1)).is_some());
}
#[test]
fn pressure_preflights_whole_vital_batch_and_exact_damage_retry() {
    let mut w = world();
    w.set_health_subscription(context(), Some(EntityId(1)))
        .unwrap();
    for i in 0..CAPACITY {
        let (before, after) = if i % 2 == 0 { (100, 99) } else { (99, 100) };
        w.apply_vital_batch(
            &[VitalMutation {
                actor: EntityId(1),
                vital: EntityVital::Health,
                before,
                after,
            }],
            None,
        )
        .unwrap();
    }
    let revision = w.combatant(EntityId(1)).unwrap().revision();
    assert!(matches!(
        w.damage_from(EntityId(1), EntityId(2), 10),
        Err(WorldError::HealthBackpressure)
    ));
    assert!(
        w.apply_vital_batch(
            &[
                VitalMutation {
                    actor: EntityId(1),
                    vital: EntityVital::Mana,
                    before: 100,
                    after: 50
                },
                VitalMutation {
                    actor: EntityId(1),
                    vital: EntityVital::Health,
                    before: 100,
                    after: 90
                }
            ],
            None
        )
        .is_err()
    );
    assert_eq!(
        w.vital(EntityId(1), EntityVital::Mana).unwrap().current,
        100
    );
    assert_eq!(w.combatant(EntityId(1)).unwrap().revision(), revision);
    w.take_health_observation();
    assert_eq!(w.damage_from(EntityId(1), EntityId(2), 10).unwrap(), 10);
    assert_eq!(w.health_observations.pending.back().unwrap().current, 90);
}
