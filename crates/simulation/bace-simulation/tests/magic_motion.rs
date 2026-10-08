mod magic_common;
use bace_gameplay_api::CastRejection;
use bace_magic::Vital;
use bace_motion::{
    ExecutionClip, MotionIntent, MotionPhysics, PreparedMotionChain, RootFrame, SourceMotionState,
    SourceMotionTransition,
};
use magic_common::*;
fn chain(motion: u32, root: f32) -> Arc<PreparedMotionChain> {
    chain_in_style(motion, root, 0x80000049)
}
fn chain_in_style(motion: u32, root: f32, style: u32) -> Arc<PreparedMotionChain> {
    let ready = SourceMotionState {
        style,
        substate: 0x41000003,
        speed: 1.0,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(
            vec![ExecutionClip {
                animation: 0x03000002,
                frame_count: 2,
                low: 0,
                high: 1,
                framerate: 30.0,
                frames: vec![RootFrame::default(); 2].into(),
                hooks: vec![].into(),
            }],
            0,
            0,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            ready.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: ready,
            after: ready,
            continues_cycle: true,
        })
        .unwrap(),
    );
    Arc::new(
        PreparedMotionChain::prepare(
            vec![
                ExecutionClip {
                    animation: 0x03000001,
                    frame_count: 10,
                    low: 0,
                    high: 9,
                    framerate: 30.0,
                    frames: vec![
                        RootFrame {
                            translation: Vec3::new(root, 0.0, 0.0),
                            heading: 0.0
                        };
                        10
                    ]
                    .into(),
                    hooks: vec![bace_motion::MotionHook {
                        frame: 5,
                        kind: 1,
                        direction: 1,
                        payload: bace_motion::MotionHookPayload::Id(99),
                    }]
                    .into(),
                },
                ExecutionClip {
                    animation: 0x03000002,
                    frame_count: 2,
                    low: 0,
                    high: 1,
                    framerate: 30.0,
                    frames: vec![RootFrame::default(); 2].into(),
                    hooks: vec![].into(),
                },
            ],
            1,
            1,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            motion,
            2.0,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: ready,
            after: ready,
            continues_cycle: false,
        })
        .unwrap()
        .with_stop_chain(stop)
        .unwrap(),
    )
}
#[test]
fn world_sequence_completion_and_legal_slide_ignore_the_fixture_timer() {
    let mut k = kernel(32);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures[0].motion_chain = Some(chain(s.gestures[0].gesture.motion, 0.0));
    s.gestures[0].duration_seconds = 0.0001;
    k.register_magic_spell(s).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..3 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    let epoch = k.world().body(EntityId(1)).unwrap().accepted().epoch();
    k.enqueue(Command::Movement {
        actor: EntityId(1),
        epoch,
        sequence: 1,
        intent: MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
    })
    .unwrap();
    for _ in 0..3 {
        step(&mut k);
    }
    assert!(
        k.world().body(EntityId(1)).unwrap().accepted().position().x > 0.1,
        "qualified zero-root action must preserve accepted slide motion"
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    for _ in 0..10 {
        step(&mut k);
    }
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
    while k.take_cast_outcome().is_some() {}
    assert!(k.world().can_retire_actor_motion(EntityId(1)));
    let owned = k
        .take_player_state(CharacterBinding {
            session: SessionId(7),
            account: AccountId(1),
            actor: EntityId(1),
        })
        .unwrap();
    assert_eq!(owned.world.unwrap().vitals[2].unwrap().current, 90);
    assert!(!k.world().motion_busy(EntityId(1)));
    assert!(!k.world().has_motion_state());
}
#[test]
fn player_root_bearing_program_is_rejected_before_motion_or_resources() {
    let mut k = kernel(32);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures[0].motion_chain = Some(chain(s.gestures[0].gesture.motion, 0.1));
    k.register_magic_spell(s).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
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
fn missing_cast_motion_assets_do_not_use_a_duration_as_production_completion() {
    let mut k = kernel_fixture_mode(32, false, false, false);
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
fn cancellation_preserves_active_action_then_ready_without_spending_resources() {
    let mut k = kernel(32);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures[0].motion_chain = Some(chain(s.gestures[0].gesture.motion, 0.0));
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
    assert!(k.world().motion_busy(EntityId(1)));
    k.take_cast_outcome().unwrap();
    k.enqueue(Command::Cast {
        context: context(2),
        request: CastRequest::Cancel,
    })
    .unwrap();
    let stopped = step(&mut k);
    assert_eq!(
        stopped
            .iter()
            .filter(|e| matches!(
                e,
                MagicEvent::MotionStopped {
                    style: 0x80000049,
                    substate: 0x41000003,
                    speed: 1.0,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(bace_gameplay_api::CastChange::Cancelled { .. })
    ));
    assert!(
        k.world().motion_busy(EntityId(1)),
        "source action cursor must survive queued stop"
    );
    let mut retained_hooks = 0;
    for _ in 0..16 {
        let events = step(&mut k);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MagicEvent::MotionStopped { .. }))
        );
        retained_hooks += events
            .iter()
            .filter(|event| matches!(event, MagicEvent::MotionHook { frame: 5, .. }))
            .count();
    }
    assert_eq!(
        retained_hooks, 1,
        "source action hook survives EndCast exactly once"
    );
    assert!(!k.world().motion_busy(EntityId(1)));
    assert_eq!(
        k.world().source_motion_state(EntityId(1)).unwrap().substate,
        0x41000003
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    assert!(k.take_cast_outcome().is_none());
}

#[test]
fn requested_peace_integer_does_not_replace_accepted_magic_style() {
    let mut k = kernel(32);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures[0].motion_chain = Some(chain(s.gestures[0].gesture.motion, 0.0));
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
    k.take_cast_outcome();
    k.enqueue(Command::Combat {
        context: context(2),
        request: bace_gameplay_api::CombatRequest::ChangeMode(1),
    })
    .unwrap();
    for _ in 0..16 {
        assert!(
            !step(&mut k)
                .iter()
                .any(|e| matches!(e, MagicEvent::Fizzle { .. }))
        );
    }
    assert!(k.take_combat_outcome().unwrap().result.is_ok());
    assert_eq!(k.world().combatant(EntityId(1)).unwrap().mode(), 1);
    assert_eq!(
        k.world().source_motion_state(EntityId(1)).unwrap().style,
        0x80000049
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(bace_gameplay_api::CastChange::Completed { .. })
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
fn completed_effect_retains_one_stop_projection_behind_full_event_outbox() {
    let mut k = kernel(2);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures[0].motion_chain = Some(chain(s.gestures[0].gesture.motion, 0.0));
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
    k.take_cast_outcome();
    // Drain the action hook, then stop draining before the effect's two vital
    // outputs fill the bounded outbox.
    for _ in 0..5 {
        step(&mut k);
    }
    for _ in 0..16 {
        k.step().unwrap();
    }
    assert_eq!(k.pending_magic_events(), 2);
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
    assert!(k.take_cast_outcome().is_none());
    let mut vitals = 0;
    while let Some(event) = k.take_magic_event() {
        vitals += usize::from(matches!(event, MagicEvent::Vital { .. }));
    }
    assert_eq!(vitals, 2);
    let events = step(&mut k);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                MagicEvent::MotionStopped {
                    style: 0x80000049,
                    substate: 0x41000003,
                    speed: 1.0,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(bace_gameplay_api::CastChange::Completed { .. })
    ));
    assert!(
        k.magic_recovery(EntityId(1)).unwrap().last_success_age > 0.2,
        "outbox retry must preserve original release time"
    );
    for _ in 0..8 {
        assert!(!step(&mut k).iter().any(|e| matches!(
            e,
            MagicEvent::MotionStopped { .. } | MagicEvent::Vital { .. }
        )));
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
fn accepted_noncombat_program_fizzles_even_when_numeric_mode_is_magic() {
    let mut k = kernel(32);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures[0].motion_chain = Some(chain_in_style(
        s.gestures[0].gesture.motion,
        0.0,
        0x8000003d,
    ));
    k.register_magic_spell(s).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    let events = step(&mut k);
    assert_eq!(k.world().combatant(EntityId(1)).unwrap().mode(), 8);
    assert_eq!(
        k.world().source_motion_state(EntityId(1)).unwrap().style,
        0x8000003d
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, MagicEvent::Fizzle { .. }))
            .count(),
        1
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        95
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(bace_gameplay_api::CastChange::Started { .. })
    ));
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(bace_gameplay_api::CastChange::Completed { .. })
    ));
}
