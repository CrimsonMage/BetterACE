//! Numeric modes below are explicit synthetic fixtures. Authentic casts consume
//! accepted interpreter style through World.source_motion_state, not this integer.
mod magic_common;
use bace_gameplay_api::{CastChange, CastOrigin, CombatRequest};
use bace_magic::Vital;
use magic_common::*;
#[test]
fn player_peace_update_fizzles_once_without_components_or_normal_spell_cost() {
    let mut k = kernel_with_components(32, true);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.components = vec![(500, 1)];
    s.component_modifiers = vec![(500, 1.0)];
    s.component_loss = 1.0;
    k.register_magic_spell(s).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Started { .. })
    ));
    k.enqueue(Command::Combat {
        context: context(2),
        request: CombatRequest::ChangeMode(1),
    })
    .unwrap();
    let events = step(&mut k);
    assert!(k.take_combat_outcome().unwrap().result.is_ok());
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, MagicEvent::Fizzle { .. }))
            .count(),
        1
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        95
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert!(k.take_inventory_proposal().is_none());
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Completed { .. })
    ));
    assert_eq!(
        k.magic_recovery(EntityId(1)).unwrap().last_success_school,
        None
    );
    for _ in 0..8 {
        assert!(
            !step(&mut k)
                .iter()
                .any(|e| matches!(e, MagicEvent::Fizzle { .. }))
        );
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        95
    );
}
#[test]
fn melee_mode_does_not_blanket_cancel_cast_and_instant_server_effect_ignores_peace() {
    let mut k = kernel(32);
    k.register_magic_spell(spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
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
    k.take_cast_outcome();
    k.enqueue(Command::Combat {
        context: context(2),
        request: CombatRequest::ChangeMode(2),
    })
    .unwrap();
    for _ in 0..8 {
        assert!(
            !step(&mut k)
                .iter()
                .any(|e| matches!(e, MagicEvent::Fizzle { .. }))
        );
    }
    assert!(k.take_combat_outcome().unwrap().result.is_ok());
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Completed { .. })
    ));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    k.enqueue(Command::Combat {
        context: context(3),
        request: CombatRequest::ChangeMode(1),
    })
    .unwrap();
    step(&mut k);
    k.cast_from_server(
        CastOrigin::Emote {
            actor: EntityId(1),
            event: 1,
            instant: true,
        },
        CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    )
    .unwrap();
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
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
    assert!(
        !step(&mut k)
            .iter()
            .any(|e| matches!(e, MagicEvent::Fizzle { .. }))
    );
}

#[test]
fn final_motion_callback_can_finish_before_same_tick_peace_update() {
    let mut k = kernel(32);
    k.register_magic_spell(spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
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
    k.take_cast_outcome();
    step(&mut k);
    // The synthetic server-completion fixture is due at0.1s; this command is
    // followed by that callback before the owning Update pass, as in GDLE.
    k.enqueue(Command::Combat {
        context: context(2),
        request: CombatRequest::ChangeMode(1),
    })
    .unwrap();
    let events = step(&mut k);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MagicEvent::Fizzle { .. }))
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Completed { .. })
    ));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
}
#[test]
fn skill_fizzle_emits_native_effect_and_specific_error_once_under_pressure() {
    let mut kernel = kernel(2);
    let mut prepared = spell(
        100,
        SpellEffect::Boost {
            vital: bace_magic::Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    prepared.spell.power = 10_000;
    kernel.register_magic_spell(prepared).unwrap();
    kernel
        .enqueue(Command::Cast {
            context: context(1),
            request: CastRequest::Targeted {
                target: EntityId(1),
                spell: 100,
            },
        })
        .unwrap();
    let mut fizzles = 0;
    let mut terminal = None;
    for _ in 0..40 {
        for event in step(&mut kernel) {
            if let MagicEvent::Fizzle {
                intensity,
                movement_incarnation,
                ..
            } = event
            {
                assert_eq!(intensity.to_bits(), 0x3f0af0a2);
                assert!(movement_incarnation.is_none());
                fizzles += 1;
            }
        }
        while let Some(outcome) = kernel.take_cast_outcome() {
            if !matches!(
                outcome.result,
                Ok(bace_gameplay_api::CastChange::Started { .. })
            ) {
                terminal = Some(outcome.result);
            }
        }
    }
    assert_eq!(
        terminal,
        Some(Err(bace_gameplay_api::CastRejection::Fizzled))
    );
    assert_eq!(fizzles, 1);
    assert_eq!(
        kernel
            .world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        95
    );
    assert_eq!(
        kernel
            .world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
}
