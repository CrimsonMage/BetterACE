mod magic_common;
use bace_gameplay_api::{CastChange, CastRejection};
use bace_magic::Vital;
use bace_motion::MotionIntent;
use magic_common::*;
#[test]
fn authenticated_heal_releases_after_motion_and_debits_mana_once() {
    let mut k = kernel(16);
    k.register_magic_spell(spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 20,
            maximum: 20,
        },
    ))
    .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Started { .. })
    ));
    for _ in 0..5 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        70
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Completed { .. })
    ));
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.take_cast_outcome().unwrap().result,
        Err(CastRejection::StaleSequence)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        70
    );
}
#[test]
fn self_mana_boost_uses_post_cost_state_and_legal_slide_does_not_install_client_pose() {
    let mut k = kernel(16);
    k.register_magic_spell(spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Mana,
            minimum: 20,
            maximum: 20,
        },
    ))
    .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    k.enqueue(Command::Movement {
        actor: EntityId(1),
        epoch: 0,
        sequence: 1,
        intent: MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
    })
    .unwrap();
    for _ in 0..5 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    assert!(k.world().body(EntityId(1)).unwrap().accepted().position().x > 0.0);
}
#[test]
fn self_target_ring_projectiles_follow_gdle_layout_and_impact_exactly_once() {
    // Eight-shot GDLE rings straddle the caster's heading by 22.5 degrees.
    // Align one ray with the target instead of relying on the old ACE center ray.
    let mut k = kernel_fixture_heading(32, false, false, true, std::f32::consts::PI / 8.0);
    k.register_magic_spell(spell(
        101,
        SpellEffect::Projectile(shot(ProjectileShape::Ring, 8)),
    ))
    .unwrap();
    for id in 1000..1008 {
        k.supply_projectile_id(EntityId(id)).unwrap();
    }
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 101,
        },
    })
    .unwrap();
    let mut spawned = Vec::new();
    for _ in 0..20 {
        for event in step(&mut k) {
            if let MagicEvent::ProjectileCreated { actor, .. } = event {
                spawned.push(actor);
            }
        }
    }
    assert_eq!(spawned.len(), 8);
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        30
    );
    for _ in 0..20 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        30
    );
}
#[test]
fn strike_launches_beyond_target_and_returns_to_hit_once() {
    let mut k = kernel(32);
    k.register_magic_spell(spell(
        102,
        SpellEffect::Projectile(shot(ProjectileShape::Strike, 1)),
    ))
    .unwrap();
    k.supply_projectile_id(EntityId(1000)).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 102,
        },
    })
    .unwrap();
    let mut launched = None;
    for _ in 0..20 {
        for event in step(&mut k) {
            if let MagicEvent::ProjectileCreated {
                position, velocity, ..
            } = event
            {
                launched = Some((position, velocity));
            }
        }
    }
    let (position, velocity) = launched.expect("source-qualified strike release");
    assert!(position.y < 5.0 && position.y > 3.0);
    assert!(velocity.y > 0.0);
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        30
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    for _ in 0..20 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        30
    );
}
#[test]
fn unsupported_strike_self_target_refuses_before_resources() {
    let mut k = kernel(16);
    k.register_magic_spell(spell(
        102,
        SpellEffect::Projectile(shot(ProjectileShape::Strike, 1)),
    ))
    .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 102,
        },
    })
    .unwrap();
    assert!(step(&mut k).is_empty());
    assert_eq!(
        k.take_cast_outcome().unwrap().result,
        Err(CastRejection::MissingAssets)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
}
#[test]
fn failed_multishot_admission_rolls_back_every_projectile_and_announcement() {
    let mut k = kernel(16);
    let mut spec = shot(ProjectileShape::Wall, 2);
    spec.padding.x = 500.0;
    k.register_magic_spell(spell(101, SpellEffect::Projectile(spec)))
        .unwrap();
    for id in [1000, 1001] {
        k.supply_projectile_id(EntityId(id)).unwrap();
    }
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Untargeted { spell: 101 },
    })
    .unwrap();
    let mut announcements = 0;
    for _ in 0..8 {
        announcements += step(&mut k)
            .into_iter()
            .filter(|e| matches!(e, MagicEvent::ProjectileCreated { .. }))
            .count();
    }
    assert_eq!(announcements, 0);
    assert!(k.world().projectile(EntityId(1000)).is_none());
    assert!(k.world().projectile(EntityId(1001)).is_none());
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
}
