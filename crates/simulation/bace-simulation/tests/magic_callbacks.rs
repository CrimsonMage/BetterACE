//! Synthetic authored clips qualify integration timing, not proprietary geometry.
mod magic_common;
use bace_gameplay_api::{CastChange, CastOrigin};
use bace_magic::Vital;
use bace_motion::{
    ExecutionClip, MotionHook, MotionHookPayload, MotionIntent, MotionPhysics, PreparedMotionChain,
    RootFrame, SourceMotionState, SourceMotionTransition,
};
use magic_common::*;
fn state(substate: u32, speed: f32) -> SourceMotionState {
    SourceMotionState {
        style: 0x80000049,
        substate,
        speed,
    }
}
fn clip(animation: u32, frames: u32, rate: f32, hooks: Vec<MotionHook>) -> ExecutionClip {
    ExecutionClip {
        animation,
        frame_count: frames,
        low: 0,
        high: frames as i32 - 1,
        framerate: rate,
        frames: vec![RootFrame::default(); frames as usize].into(),
        hooks: hooks.into(),
    }
}
fn program(motion: u32, zero: bool, repeat: bool, hook: bool) -> Arc<PreparedMotionChain> {
    let ready = state(0x41000003, 1.0);
    let blast = state(0x4000002b, 2.0);
    let after = if zero { blast } else { ready };
    let before = if repeat { blast } else { ready };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(
            vec![clip(0x03000002, 2, 30.0, vec![])],
            0,
            0,
            physics,
            ready.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: after,
            after: ready,
            continues_cycle: !zero,
        })
        .unwrap(),
    );
    let clips = if zero {
        vec![clip(0x03000003, 2, 0.0, vec![])]
    } else {
        vec![
            clip(
                motion,
                4,
                30.0,
                if hook {
                    vec![MotionHook {
                        frame: 0,
                        kind: 1,
                        direction: 1,
                        payload: MotionHookPayload::Id(99),
                    }]
                } else {
                    vec![]
                },
            ),
            clip(0x03000002, 2, 30.0, vec![]),
        ]
    };
    Arc::new(
        PreparedMotionChain::prepare(
            clips,
            usize::from(!zero),
            usize::from(!zero),
            physics,
            motion,
            2.0,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before,
            after,
            continues_cycle: repeat,
        })
        .unwrap()
        .with_stop_chain(stop)
        .unwrap(),
    )
}
fn prepared(zero_count: usize) -> PreparedMagicSpell {
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.gestures = vec![PreparedCastGesture {
        gesture: CastGesture {
            motion: 0x13000132,
            minimum_seconds: 0.0,
        },
        duration_seconds: 0.0,
        motion_chain: Some(program(0x13000132, false, false, false)),
    }];
    for i in 0..zero_count {
        s.gestures.push(PreparedCastGesture {
            gesture: CastGesture {
                motion: 0x4000002b,
                minimum_seconds: 0.0,
            },
            duration_seconds: 0.0,
            motion_chain: Some(program(0x4000002b, true, i > 0, false)),
        });
    }
    s
}
fn begin(k: &mut Kernel) {
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
}
#[test]
fn physics_callback_releases_all_zero_successors_in_same_phase_without_extra_movement() {
    for zeros in [2, 31] {
        let mut burst = kernel(128);
        let mut baseline = kernel(128);
        burst.register_magic_spell(prepared(zeros)).unwrap();
        baseline.register_magic_spell(prepared(0)).unwrap();
        begin(&mut burst);
        begin(&mut baseline);
        step(&mut burst);
        step(&mut baseline);
        for k in [&mut burst, &mut baseline] {
            let epoch = k.world().actor_state(EntityId(1)).unwrap().1.epoch();
            k.enqueue(Command::Movement {
                actor: EntityId(1),
                epoch,
                sequence: 1,
                intent: MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
            })
            .unwrap();
        }
        let mut completed = false;
        for _ in 0..12 {
            let events = step(&mut burst);
            step(&mut baseline);
            assert_eq!(
                burst.world().actor_state(EntityId(1)).unwrap(),
                baseline.world().actor_state(EntityId(1)).unwrap(),
                "callbacks cannot advance accepted physics again"
            );
            if events.iter().any(|e| {
                matches!(
                    e,
                    MagicEvent::Motion {
                        motion: 0x4000002b,
                        ..
                    }
                )
            }) {
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(
                            e,
                            MagicEvent::Motion {
                                motion: 0x4000002b,
                                ..
                            }
                        ))
                        .count(),
                    zeros
                );
                assert_eq!(
                    burst
                        .world()
                        .vital(EntityId(1), EntityVital::Health)
                        .unwrap()
                        .current,
                    60
                );
                assert_eq!(
                    baseline
                        .world()
                        .vital(EntityId(1), EntityVital::Health)
                        .unwrap()
                        .current,
                    60
                );
                assert_eq!(
                    burst
                        .world()
                        .vital(EntityId(1), EntityVital::Mana)
                        .unwrap()
                        .current,
                    90
                );
                completed = true;
                break;
            }
        }
        assert!(completed);
        for _ in 0..8 {
            assert!(
                !step(&mut burst)
                    .iter()
                    .any(|e| matches!(e, MagicEvent::Vital { .. }))
            );
        }
    }
}
#[test]
fn initial_zero_program_debits_creature_mana_then_waits_for_first_world_phase() {
    for origin in [
        CastOrigin::Emote {
            actor: EntityId(1),
            event: 1,
            instant: false,
        },
        CastOrigin::Emote {
            actor: EntityId(2),
            event: 1,
            instant: false,
        },
        CastOrigin::Monster {
            actor: EntityId(2),
            event: 1,
        },
    ] {
        // Spending the entire pool also proves release validation does not
        // require another base-mana payment after the up-front debit.
        for base_mana in [10, 100] {
            let mut k = kernel(32);
            let mut s = prepared(1);
            s.spell.base_mana = base_mana;
            s.gestures.remove(0);
            k.register_magic_spell(s).unwrap();
            let actor = origin.actor();
            k.cast_from_server(
                origin,
                CastRequest::Targeted {
                    target: actor,
                    spell: 100,
                },
            )
            .unwrap();
            assert_eq!(
                k.world().vital(actor, EntityVital::Health).unwrap().current,
                50
            );
            // GDLE CreatureBeginCast charges before BeginNextMotion. Its
            // player-owned emote double charge is intentionally corrected;
            // this is not a claim that GDLE has an IsCreatureCast guard.
            assert_eq!(
                k.world().vital(actor, EntityVital::Mana).unwrap().current,
                100 - base_mana
            );
            assert!(k.take_server_cast_outcome().is_none());
            step(&mut k);
            assert_eq!(
                k.world().vital(actor, EntityVital::Health).unwrap().current,
                60
            );
            assert_eq!(
                k.world().vital(actor, EntityVital::Mana).unwrap().current,
                100 - base_mana,
                "the motion callback must not charge prepaid creature mana again: {origin:?}"
            );
            assert!(matches!(
                k.take_server_cast_outcome().unwrap().result,
                Ok(CastChange::Completed { .. })
            ));
        }
    }
}
#[test]
fn ordinary_player_zero_program_charges_once_at_release() {
    let mut k = kernel(32);
    let mut s = prepared(1);
    s.gestures.remove(0);
    k.register_magic_spell(s).unwrap();
    begin(&mut k);
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    step(&mut k);
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
    for _ in 0..3 {
        step(&mut k);
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
fn zero_callback_backpressure_retains_cast_and_spends_once() {
    let mut k = kernel(2);
    k.register_magic_spell(prepared(2)).unwrap();
    begin(&mut k);
    step(&mut k);
    k.take_cast_outcome();
    for _ in 0..12 {
        k.step().unwrap();
    }
    assert_eq!(k.pending_magic_events(), 2);
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
    let mut gestures = 0;
    while let Some(e) = k.take_magic_event() {
        gestures += usize::from(matches!(
            e,
            MagicEvent::Motion {
                motion: 0x4000002b,
                ..
            }
        ));
    }
    assert_eq!(gestures, 2);
    let mut vitals = 0;
    let mut stops = 0;
    let mut finished = 0;
    for _ in 0..8 {
        for e in step(&mut k) {
            vitals += usize::from(matches!(e, MagicEvent::Vital { .. }));
            stops += usize::from(matches!(e, MagicEvent::MotionStopped { .. }));
        }
        while let Some(o) = k.take_cast_outcome() {
            finished += usize::from(matches!(o.result, Ok(CastChange::Completed { .. })));
        }
    }
    assert_eq!((vitals, stops, finished), (2, 1, 1));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
}
#[test]
fn callback_appended_timed_action_does_not_advance_or_emit_its_frame_zero_hook_early() {
    let mut k = kernel(32);
    let mut s = prepared(0);
    s.gestures.push(PreparedCastGesture {
        gesture: CastGesture {
            motion: 0x13000133,
            minimum_seconds: 0.0,
        },
        duration_seconds: 0.0,
        motion_chain: Some(program(0x13000133, false, false, true)),
    });
    k.register_magic_spell(s).unwrap();
    begin(&mut k);
    let mut appended = false;
    for _ in 0..12 {
        let events = step(&mut k);
        if events.iter().any(|e| {
            matches!(
                e,
                MagicEvent::Motion {
                    motion: 0x13000133,
                    ..
                }
            )
        }) {
            assert!(!events.iter().any(|e| matches!(
                e,
                MagicEvent::MotionHook {
                    animation: 0x13000133,
                    ..
                }
            )));
            assert_eq!(
                k.world()
                    .vital(EntityId(1), EntityVital::Health)
                    .unwrap()
                    .current,
                50
            );
            appended = true;
            break;
        }
    }
    assert!(appended);
    let next = step(&mut k);
    assert_eq!(
        next.iter()
            .filter(|e| matches!(
                e,
                MagicEvent::MotionHook {
                    animation: 0x13000133,
                    frame: 0,
                    ..
                }
            ))
            .count(),
        1
    );
}

#[test]
fn pinned_creature_mana_oracle_preserves_the_corrected_player_emote_discrepancy() {
    let rows: Vec<[i32; 18]> = include_str!("fixtures/creature_mana.csv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            line.split(',')
                .map(|value| value.parse::<i32>().unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        })
        .collect();
    assert_eq!(rows.len(), 1512);
    for player in [0, 1] {
        let matching: Vec<_> = rows
            .iter()
            .filter(|row| row[..7] == [player, 1, 10, 100, 10, 0, 0])
            .collect();
        assert_eq!(matching.len(), 1);
        let original = matching[0];
        assert_eq!(original[7], 0, "source admission succeeds");
        assert_eq!(original[9], 90, "source charges before motion");
        assert_eq!(original[10], if player == 1 { 80 } else { 90 });
        assert_eq!(
            original[12], player,
            "source player-only GenerateManaCost call"
        );
        assert_eq!(original[13], 1 + player, "source requested debit count");
        assert_eq!(original[14], -10);
        assert_eq!(original[15], -10 * player);

        let actor = EntityId(if player == 1 { 1 } else { 2 });
        let mut k = kernel(32);
        let mut s = prepared(1);
        s.gestures.remove(0);
        k.register_magic_spell(s).unwrap();
        k.cast_from_server(
            CastOrigin::Emote {
                actor,
                event: 1,
                instant: false,
            },
            CastRequest::Targeted {
                target: actor,
                spell: 100,
            },
        )
        .unwrap();
        assert_eq!(
            k.world().vital(actor, EntityVital::Mana).unwrap().current,
            original[9] as u32
        );
        step(&mut k);
        assert_eq!(
            k.world().vital(actor, EntityVital::Health).unwrap().current,
            60
        );
        let corrected = k.world().vital(actor, EntityVital::Mana).unwrap().current;
        assert_eq!(corrected, 90);
        // Deliberate defect correction under BASE-01, not source equivalence:
        // only the player-owned creature route differs from the original.
        assert_eq!(corrected as i32 - original[10], 10 * player);
    }
}
