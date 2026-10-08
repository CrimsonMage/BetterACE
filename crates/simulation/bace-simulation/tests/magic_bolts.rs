//! Authoritative synthetic bolt/arc slice; authentic full-model and cross-cell
//! coverage remains with qualified World/asset tests, not these sphere fixtures.
mod magic_common;
use bace_gameplay_api::{CastChange, CastRejection};
use magic_common::*;
fn missile(k: &mut Kernel, arc: bool, tracking: bool, speed: f32) {
    let mut p = shot(
        if arc {
            ProjectileShape::Arc
        } else {
            ProjectileShape::Bolt
        },
        1,
    );
    p.gravity = if arc { 9.8 } else { 0.0 };
    p.speed = speed;
    p.tracking = tracking;
    let mut s = spell(100, SpellEffect::Projectile(p));
    s.spell.harmful = true;
    // A real recovery floor survives completion; network messages cannot advance it.
    s.gestures[0].gesture.minimum_seconds = 2.0;
    k.register_magic_spell(s).unwrap();
    for id in 50..54 {
        k.supply_projectile_id(EntityId(id)).unwrap();
    }
}
fn cast(sequence: u32) -> Command {
    Command::Cast {
        context: context(sequence),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    }
}
#[test]
fn bolt_and_arc_launch_hit_once_then_explode_and_destroy_after_half_second() {
    for arc in [false, true] {
        let mut k = kernel(32);
        missile(&mut k, arc, false, 20.0);
        k.enqueue(cast(1)).unwrap();
        let mut created = None;
        let mut explosion_tick = None;
        let mut damage = 0;
        let mut removed = None;
        for tick in 1..=50 {
            for e in step(&mut k) {
                match e {
                    MagicEvent::ProjectileCreated {
                        actor,
                        position,
                        velocity,
                        ..
                    } => {
                        assert_eq!(actor, EntityId(50));
                        assert!(created.is_none());
                        assert!(position.y > 0.69 && position.y < 0.71);
                        // A close, lower target can require a downward initial
                        // arc velocity. Independent GDLE launch vectors cover it.
                        assert!(velocity.z.is_finite());
                        created = Some(tick);
                    }
                    MagicEvent::ProjectileExploded { actor, .. } => {
                        assert_eq!(actor, EntityId(50));
                        assert!(explosion_tick.replace(tick).is_none());
                    }
                    MagicEvent::Vital {
                        actor: EntityId(2), ..
                    } => damage += 1,
                    MagicEvent::ProjectileRemoved {
                        actor: EntityId(50),
                        ..
                    } => {
                        removed = Some(tick);
                    }
                    _ => {}
                }
            }
        }
        assert!(created.is_some());
        assert_eq!(damage, 1);
        assert_eq!(
            k.world()
                .vital(EntityId(2), EntityVital::Health)
                .unwrap()
                .current,
            30
        );
        assert_eq!(removed.unwrap() - explosion_tick.unwrap(), 15);
        assert_eq!(
            k.world()
                .vital(EntityId(1), EntityVital::Mana)
                .unwrap()
                .current,
            90
        );
    }
}
#[test]
fn queued_cast_spam_cannot_release_early_duplicate_or_bypass_recovery() {
    let mut k = kernel(128);
    missile(&mut k, false, false, 20.0);
    for sequence in 1..=32 {
        k.enqueue(cast(sequence)).unwrap();
    }
    let first = step(&mut k);
    assert!(
        !first
            .iter()
            .any(|e| matches!(e, MagicEvent::ProjectileCreated { .. }))
    );
    let outcomes: Vec<_> = std::iter::from_fn(|| k.take_cast_outcome()).collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o.result, Ok(CastChange::Started { .. })))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| o.result == Err(CastRejection::Busy))
            .count(),
        31
    );
    let mut created = 0;
    for _ in 0..8 {
        created += step(&mut k)
            .iter()
            .filter(|e| matches!(e, MagicEvent::ProjectileCreated { .. }))
            .count();
        while k.take_cast_outcome().is_some() {}
    }
    assert_eq!(created, 1);
    for sequence in 33..=64 {
        k.enqueue(cast(sequence)).unwrap();
    }
    assert!(
        !step(&mut k)
            .iter()
            .any(|e| matches!(e, MagicEvent::ProjectileCreated { .. }))
    );
    while let Some(outcome) = k.take_cast_outcome() {
        assert_eq!(outcome.result, Err(CastRejection::Busy));
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
}
#[test]
fn tracking_is_frozen_lead_not_homing_and_created_velocity_is_accepted() {
    let mut k = kernel(32);
    missile(&mut k, true, true, 100.0);
    let epoch = k.world().actor_state(EntityId(2)).unwrap().1.epoch();
    k.enqueue(Command::Movement {
        actor: EntityId(2),
        epoch,
        sequence: 1,
        intent: bace_motion::MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
    })
    .unwrap();
    k.enqueue(cast(1)).unwrap();
    let mut announced = None;
    for _ in 0..8 {
        for e in step(&mut k) {
            if let MagicEvent::ProjectileCreated {
                actor, velocity, ..
            } = e
            {
                assert!(velocity.length_squared().sqrt() <= 50.00001);
                assert!(velocity.x > 0.0);
                assert_eq!(
                    k.world().projectile(actor).unwrap().body.velocity(),
                    velocity
                );
                announced = Some(velocity);
            }
        }
        if announced.is_some() {
            break;
        }
    }
    let before = announced.unwrap();
    k.enqueue(Command::Movement {
        actor: EntityId(2),
        epoch,
        sequence: 2,
        intent: bace_motion::MotionIntent::new(Vec3::new(-1.0, 0.0, 0.0), false).unwrap(),
    })
    .unwrap();
    step(&mut k);
    let after = k.world().projectile(EntityId(50)).unwrap().body.velocity();
    // GDLE's speed ceiling may rescale all axes after gravity. It preserves
    // the horizontal direction; target reversal must not steer the shot.
    assert!(after.x > 0.0);
    assert!((after.x * before.y - before.x * after.y).abs() < 0.0001);
    assert!(after.z < before.z);
}
