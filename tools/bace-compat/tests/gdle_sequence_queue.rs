//! Original pinned intrusive sequence + pending MotionTableManager queue oracle.
use bace_geometry::Vec3;
use bace_motion::{
    ExecutionClip, MotionDomain, MotionExecutionEvent, MotionPhysics, MotionPlayback,
    MotionPlaybackEvent, MotionToken, PreparedMotionChain, SourceMotionState,
    SourceMotionTransition,
};
use serde_json::Value;
use std::sync::Arc;
fn token(sequence: u64) -> MotionToken {
    MotionToken {
        domain: MotionDomain::Casting,
        owner: 1,
        sequence,
    }
}
fn state(substate: u32, speed: f32) -> SourceMotionState {
    SourceMotionState {
        style: 0x80000049,
        substate,
        speed,
    }
}
fn clip(id: u32, rate: f32) -> ExecutionClip {
    ExecutionClip {
        animation: id,
        frame_count: 4,
        low: 0,
        high: 3,
        framerate: rate,
        frames: Vec::new().into(),
        hooks: Vec::new().into(),
    }
}
fn prepare(
    clips: Vec<ExecutionClip>,
    count: usize,
    motion: u32,
    before: SourceMotionState,
    after: SourceMotionState,
    continuation: bool,
) -> Arc<PreparedMotionChain> {
    Arc::new(
        PreparedMotionChain::prepare(
            clips,
            count,
            count,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            motion,
            after.speed,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before,
            after,
            continues_cycle: continuation,
        })
        .unwrap(),
    )
}
fn check(playback: &MotionPlayback, events: &[MotionPlaybackEvent], expected: &Value) {
    let cursor = playback.cursor();
    let clip = &playback.chain().clips()[cursor.segment];
    assert_eq!(
        clip.animation,
        expected["animation"].as_u64().unwrap() as u32
    );
    assert_eq!(
        cursor.frame.to_bits(),
        expected["frame_bits"].as_u64().unwrap(),
        "{expected}"
    );
    assert_eq!(clip.framerate, expected["rate"].as_f64().unwrap() as f32);
    let completed: Vec<_> = events
        .iter()
        .filter(|e| e.event == MotionExecutionEvent::Completed)
        .map(|e| {
            if e.token.sequence == 1 {
                0x13000132u32
            } else {
                0x41000003
            }
        })
        .collect();
    let expected: Vec<_> = expected["completed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect();
    assert_eq!(completed, expected);
}
#[test]
fn original_stop_preserves_active_links_cursor_and_both_fifo_callbacks() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    for case in fixture["vectors"]["queue"]["stops"].as_array().unwrap() {
        let direction = case["direction"].as_f64().unwrap() as f32;
        let mode = case["mode"].as_u64().unwrap();
        let before = state(if mode == 0 { 0x41000003 } else { 0x4000002f }, 2.0);
        let prepared = prepare(
            vec![
                clip(1, 30.0 * direction),
                clip(2, 15.0 * direction),
                clip(3, 30.0),
            ],
            2,
            0x13000132,
            before,
            before,
            false,
        );
        let mut playback = MotionPlayback::new(prepared, token(1)).unwrap();
        let mut events = Vec::with_capacity(4096);
        for _ in 0..case["before"].as_u64().unwrap() {
            playback
                .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
                .unwrap();
        }
        check(&playback, &events, &case["initial"]);
        events.clear();
        let stop = if mode == 0 {
            prepare(
                vec![clip(3, 15.0)],
                0,
                0x41000003,
                before,
                state(0x41000003, 1.0),
                true,
            )
        } else {
            prepare(
                vec![clip(4, 30.0), clip(5, 15.0)],
                1,
                0x41000003,
                before,
                state(0x41000003, 1.0),
                false,
            )
        };
        playback = playback.append_stop(stop, token(2)).unwrap();
        check(&playback, &events, &case["stopped"]);
        for expected in case["steps"].as_array().unwrap() {
            events.clear();
            playback
                .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
                .unwrap();
            check(&playback, &events, expected);
        }
    }
}
#[test]
fn original_same_substate_rate_update_preserves_fractional_cycle_phase() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    for case in fixture["vectors"]["queue"]["rates"].as_array().unwrap() {
        let before = case["before"].as_f64().unwrap() as f32;
        let after = case["after"].as_f64().unwrap() as f32;
        let old = state(0x4000002f, before);
        let new = state(0x4000002f, after);
        let mut playback = MotionPlayback::new(
            prepare(
                vec![clip(3, 30.0 * before)],
                0,
                old.substate,
                old,
                old,
                false,
            ),
            token(1),
        )
        .unwrap();
        let mut events = Vec::with_capacity(4096);
        playback
            .advance_tagged(2.375 / f64::from(30.0 * before), &mut events, 4096)
            .unwrap();
        events.clear();
        let chain = prepare(vec![clip(3, 30.0 * after)], 0, new.substate, old, new, true);
        assert!(MotionPlayback::new(chain.clone(), token(2)).is_err());
        playback = playback.continue_cycle(chain, token(2)).unwrap();
        let changed = &case["changed"];
        assert_eq!(
            playback.cursor().frame.to_bits(),
            changed["frame_bits"].as_u64().unwrap()
        );
        assert_eq!(
            playback.chain().clips()[0].framerate,
            changed["rate"].as_f64().unwrap() as f32
        );
        playback
            .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
            .unwrap();
        assert_eq!(
            playback.cursor().frame.to_bits(),
            case["step"]["frame_bits"].as_u64().unwrap()
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event == MotionExecutionEvent::Completed)
                .count(),
            case["step"]["completed"].as_array().unwrap().len()
        );
        assert_eq!(events[0].token, token(2));
    }
}
#[test]
fn original_repeated_substate_compaction_keeps_fifo_zero_count_callbacks() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    let fixture = &fixture["vectors"]["queue"]["coalesced"];
    let ready = state(0x41000003, 1.0);
    let casting = state(0x4000002f, 1.0);
    let mut playback = MotionPlayback::new(
        prepare(
            vec![clip(1, 30.0), clip(2, 15.0), clip(3, 30.0)],
            2,
            ready.substate,
            casting,
            ready,
            false,
        ),
        token(1),
    )
    .unwrap();
    let mut events = Vec::with_capacity(4096);
    playback
        .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
        .unwrap();
    playback = playback
        .append_stop(
            prepare(
                vec![clip(4, 30.0), clip(5, 15.0)],
                1,
                casting.substate,
                ready,
                casting,
                false,
            ),
            token(2),
        )
        .unwrap();
    playback = playback
        .append_stop(
            prepare(
                vec![clip(1, 30.0), clip(3, 30.0)],
                1,
                ready.substate,
                casting,
                ready,
                false,
            ),
            token(3),
        )
        .unwrap();
    assert_eq!(playback.chain().clips().len(), 3);
    for expected in std::iter::once(&fixture["initial"]).chain(fixture["steps"].as_array().unwrap())
    {
        if expected != &fixture["initial"] {
            events.clear();
            playback
                .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
                .unwrap();
        }
        assert_eq!(
            playback.cursor().frame.to_bits(),
            expected["frame_bits"].as_u64().unwrap()
        );
        assert_eq!(
            playback.chain().clips()[playback.cursor().segment].animation,
            expected["animation"].as_u64().unwrap() as u32
        );
        let actual: Vec<_> = events
            .iter()
            .filter(|e| e.event == MotionExecutionEvent::Completed)
            .map(|e| {
                if e.token.sequence == 2 {
                    0x4000002fu32
                } else {
                    0x41000003
                }
            })
            .collect();
        let expected: Vec<_> = expected["completed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(actual, expected);
    }
}
#[test]
fn queued_callbacks_backpressure_is_atomic_for_cursor_and_output() {
    let ready = state(0x41000003, 1.0);
    let mut link = clip(1, 30.0);
    link.high = 0;
    let initial = prepare(
        vec![link, clip(3, 30.0)],
        1,
        0x13000132,
        ready,
        ready,
        false,
    );
    let mut playback = MotionPlayback::new(initial, token(1))
        .unwrap()
        .append_stop(
            prepare(vec![clip(3, 30.0)], 0, ready.substate, ready, ready, true),
            token(2),
        )
        .unwrap();
    let before = playback.cursor();
    let mut output = Vec::with_capacity(1);
    assert!(
        playback
            .advance_tagged(f64::from(1.0f32 / 30.0), &mut output, 1)
            .is_err()
    );
    assert_eq!(playback.cursor(), before);
    assert!(output.is_empty());
    let mut output = Vec::with_capacity(2);
    playback
        .advance_tagged(f64::from(1.0f32 / 30.0), &mut output, 2)
        .unwrap();
    assert_eq!(
        output.iter().map(|e| e.token).collect::<Vec<_>>(),
        vec![token(1), token(2)]
    );
}
#[test]
fn original_cast_gate_allows_retained_substate_and_ready_links_before_they_complete() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    let fixture = &fixture["vectors"]["queue"]["substate_then_action"];
    let ready = state(0x41000003, 1.0);
    let casting = state(0x4000002f, 1.0);
    let mut playback = MotionPlayback::new(
        prepare(
            vec![clip(1, 30.0), clip(2, 15.0), clip(3, 30.0)],
            2,
            casting.substate,
            ready,
            casting,
            false,
        ),
        token(1),
    )
    .unwrap();
    let mut events = Vec::with_capacity(4096);
    playback
        .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
        .unwrap();
    playback = playback
        .append_stop(
            prepare(
                vec![clip(4, 30.0), clip(5, 15.0)],
                1,
                ready.substate,
                casting,
                ready,
                false,
            ),
            token(2),
        )
        .unwrap();
    assert!(!playback.cursor().completed);
    assert_eq!(
        !playback.has_pending_action(),
        fixture["admitted"].as_bool().unwrap()
    );
    assert_eq!(
        playback.cursor().frame.to_bits(),
        fixture["stopped"]["frame_bits"].as_u64().unwrap()
    );
    playback = playback
        .append_stop(
            prepare(
                vec![clip(4, 30.0), clip(5, 15.0)],
                1,
                0x13000132,
                ready,
                ready,
                false,
            ),
            token(3),
        )
        .unwrap();
    assert_eq!(
        playback.has_pending_action(),
        fixture["blocked_after_action"].as_bool().unwrap()
    );
    assert_eq!(
        playback.cursor().frame.to_bits(),
        fixture["admitted_frame"]["frame_bits"].as_u64().unwrap()
    );
    for expected in fixture["steps"].as_array().unwrap() {
        events.clear();
        playback
            .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
            .unwrap();
        assert_eq!(
            playback.cursor().frame.to_bits(),
            expected["frame_bits"].as_u64().unwrap()
        );
        assert_eq!(
            playback.chain().clips()[playback.cursor().segment].animation,
            expected["animation"].as_u64().unwrap() as u32
        );
        let actual: Vec<_> = events
            .iter()
            .filter(|e| e.event == MotionExecutionEvent::Completed)
            .map(|e| match e.token.sequence {
                1 => 0x4000002fu32,
                2 => 0x41000003,
                _ => 0x13000132,
            })
            .collect();
        let expected: Vec<_> = expected["completed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(actual, expected);
    }
    assert!(!playback.has_pending_action());
}
#[test]
fn original_reentrant_callbacks_complete_appended_zero_substates_without_another_frame_update() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    for case in fixture["vectors"]["queue"]["reentrant"].as_array().unwrap() {
        let ready = state(0x41000003, 1.0);
        let pose = state(0x4000002f, 1.0);
        let mut events = Vec::with_capacity(4096);
        let mut completed = Vec::new();
        let mut playback = if case["trigger"] == 0 {
            let mut link = clip(1, 30.0);
            link.high = 0;
            let mut playback = MotionPlayback::new(
                prepare(
                    vec![link, clip(2, 30.0)],
                    1,
                    0x13000132,
                    ready,
                    ready,
                    false,
                ),
                token(1),
            )
            .unwrap();
            playback
                .advance_tagged(f64::from(1.0f32 / 30.0), &mut events, 4096)
                .unwrap();
            assert_eq!(events.len(), 1);
            completed.push(playback.chain().motion);
            for (sequence, id, after) in [(2, 3, pose), (3, 4, state(0x40000030, 1.0))] {
                let before = playback.chain().source_transition().unwrap().after;
                playback = MotionPlayback::new(
                    prepare(vec![clip(id, 0.0)], 0, after.substate, before, after, false),
                    token(sequence),
                )
                .unwrap();
                events.clear();
                playback.advance_tagged(0.0, &mut events, 4096).unwrap();
                assert_eq!(events.len(), 1);
                completed.push(playback.chain().motion);
            }
            playback
        } else {
            let initial = prepare(vec![clip(3, 30.0)], 0, pose.substate, pose, pose, false);
            let mut playback = MotionPlayback::new(initial, token(1)).unwrap();
            playback
                .advance_tagged(2.375 / 30.0, &mut events, 4096)
                .unwrap();
            for sequence in 2..=4 {
                let next = prepare(vec![clip(3, 30.0)], 0, pose.substate, pose, pose, true);
                playback = playback.continue_cycle(next, token(sequence)).unwrap();
                events.clear();
                playback.advance_tagged(0.0, &mut events, 4096).unwrap();
                assert_eq!(events.len(), 1);
                completed.push(playback.chain().motion);
            }
            playback
        };
        let expected = &case["result"];
        assert_eq!(
            playback.cursor().frame.to_bits(),
            expected["frame_bits"].as_u64().unwrap()
        );
        assert_eq!(
            playback.chain().clips()[playback.cursor().segment].animation,
            expected["animation"].as_u64().unwrap() as u32
        );
        assert_eq!(
            completed,
            expected["completed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>()
        );
        events.clear();
        playback.advance_tagged(0.0, &mut events, 4096).unwrap();
        assert!(events.is_empty());
    }
}
