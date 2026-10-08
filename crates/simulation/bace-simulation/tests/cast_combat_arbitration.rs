#[allow(dead_code, unused_imports)]
mod magic_common;
use bace_gameplay_api::{CombatRejection, CombatRequest};
use bace_magic::Vital;
use magic_common::*;
#[test]
fn physical_requests_cannot_replace_a_casting_motion_owner() {
    let mut k = kernel(16);
    let mut prepared = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 20,
            maximum: 20,
        },
    );
    prepared.gestures[0].duration_seconds = 0.5;
    k.register_magic_spell(prepared).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    k.enqueue(Command::Combat {
        context: context(2),
        request: CombatRequest::ChangeMode(2),
    })
    .unwrap();
    k.enqueue(Command::Combat {
        context: context(3),
        request: CombatRequest::TargetedMelee {
            target: EntityId(2),
            height: 2,
            power: 0.5,
        },
    })
    .unwrap();
    step(&mut k);
    assert!(k.take_combat_outcome().unwrap().result.is_ok());
    assert_eq!(
        k.take_combat_outcome().unwrap().result,
        Err(CombatRejection::Busy)
    );
    k.enqueue(Command::Combat {
        context: context(4),
        request: CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 2,
            accuracy: 0.5,
        },
    })
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.take_combat_outcome().unwrap().result,
        Err(CombatRejection::Busy)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    for _ in 0..20 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        70
    );
    k.enqueue(Command::Combat {
        context: context(5),
        request: CombatRequest::TargetedMelee {
            target: EntityId(2),
            height: 2,
            power: 0.5,
        },
    })
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.take_combat_outcome().unwrap().result,
        Err(CombatRejection::OutOfRange)
    );
}
