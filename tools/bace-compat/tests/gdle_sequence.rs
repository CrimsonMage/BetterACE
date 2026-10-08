use bace_geometry::Vec3;
use bace_motion::{
    ExecutionClip, MotionDomain, MotionExecutionEvent, MotionHook, MotionHookPayload,
    MotionPhysics, MotionPlayback, MotionToken, PreparedMotionChain,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

struct Source {
    default: u32,
    data: BTreeMap<u32, (u32, usize, u8)>,
    cycles: BTreeMap<u32, u32>,
    links: BTreeMap<(u32, u32), u32>,
}
impl bace_motion::MotionTableSource for Source {
    type Data = (u32, usize, u8);
    fn default_style(&self) -> u32 {
        0x80000049
    }
    fn bitfield(&self, data: &Self::Data) -> u8 {
        data.2
    }
    fn default_motion(&self, style: u32) -> Option<u32> {
        (style == 0x80000049 || style == 0x8000003c).then_some(self.default)
    }
    fn cycle(&self, key: u32) -> Option<&Self::Data> {
        self.cycles.get(&key).and_then(|id| self.data.get(id))
    }
    fn link(&self, key: u32, motion: u32) -> Option<&Self::Data> {
        self.links
            .get(&(key, motion))
            .and_then(|id| self.data.get(id))
    }
    fn animation_count(&self, data: &Self::Data) -> usize {
        data.1
    }
}
#[test]
fn original_gdle_action_links_order_and_completion_count() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    let number = |v: &Value| v.as_u64().unwrap() as u32;
    for case in fixture["vectors"]["chains"].as_array().unwrap() {
        let source = Source {
            default: number(&case["default"]),
            data: case["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    (
                        number(&v[0]),
                        (number(&v[0]), number(&v[1]) as usize, number(&v[2]) as u8),
                    )
                })
                .collect(),
            cycles: case["cycles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (number(&v[0]), number(&v[1])))
                .collect(),
            links: case["links"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| ((number(&v[0]), number(&v[1])), number(&v[2])))
                .collect(),
        };
        let actual = bace_motion::resolve_sequence(
            &source,
            bace_motion::ActionChainRequest {
                style: number(&case["style"]),
                current: number(&case["current"]),
                current_speed: case["current_speed"].as_f64().unwrap() as f32,
                action: number(&case["action"]),
                action_speed: case["action_speed"].as_f64().unwrap() as f32,
            },
        );
        assert_eq!(
            actual.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(actual) = actual {
            assert_eq!(
                actual.completion_clips,
                number(&case["completion"]) as usize,
                "{case}"
            );
            let parts: Vec<_> = actual.parts.iter().map(|p| (p.data.0, p.speed)).collect();
            let expected: Vec<_> = case["parts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (number(&v[0]), v[1].as_f64().unwrap() as f32))
                .collect();
            if actual.continues_cycle {
                assert!(expected.is_empty());
                assert_eq!(parts.len(), 1);
                assert_eq!(actual.completion_clips, 0);
            } else {
                assert_eq!(parts, expected, "{case}");
            }
            if number(&case["action"]) & 0x10000000 != 0 {
                assert_eq!(
                    actual.first_cyclic,
                    actual.completion_clips + actual.cycle.1 - 1
                );
            } else {
                assert_eq!(actual.first_cyclic, actual.completion_clips);
            }
        }
    }
}
fn chain(speed: f32, direction: f32) -> Arc<PreparedMotionChain> {
    let clips = (0..3)
        .map(|segment| ExecutionClip {
            animation: segment + 1,
            frame_count: 4,
            low: if segment == 0 { 1 } else { 0 },
            high: 3,
            framerate: if segment == 1 { 15.0 } else { 30.0 } * speed * direction,
            frames: Vec::new().into(),
            hooks: (0..4)
                .flat_map(|frame| {
                    (0..4).map(move |h| MotionHook {
                        frame,
                        kind: 1,
                        direction: match h {
                            0 => 0,
                            1 => 1,
                            2 => -1,
                            _ => -2,
                        },
                        payload: MotionHookPayload::Id((segment + 1) * 100 + frame * 10 + h),
                    })
                })
                .collect::<Vec<_>>()
                .into(),
        })
        .collect();
    Arc::new(
        PreparedMotionChain::prepare(
            clips,
            2,
            2,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            0x13000132,
            speed * direction,
        )
        .unwrap(),
    )
}
#[test]
fn original_gdle_f32_quantum_double_cursor_hooks_and_completion() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    assert_eq!(
        fixture["commit"],
        "353cbab52ef7da2b7063bc3e3f008461d8531693"
    );
    for case in fixture["vectors"]["execution"].as_array().unwrap() {
        let mut playback = MotionPlayback::new(
            chain(
                case["speed"].as_f64().unwrap() as f32,
                case["direction"].as_f64().unwrap() as f32,
            ),
            MotionToken {
                domain: MotionDomain::Casting,
                owner: 1,
                sequence: 1,
            },
        )
        .unwrap();
        let mut output = Vec::with_capacity(4096);
        let mut completed = 0;
        for (tick, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            output.clear();
            playback
                .advance(
                    f64::from(f32::from_bits(case["dt_bits"].as_u64().unwrap() as u32)),
                    &mut output,
                    4096,
                )
                .unwrap();
            let actual: Vec<_> = output
                .iter()
                .map(|event| match event {
                    MotionExecutionEvent::Hook {
                        payload: MotionHookPayload::Id(id),
                        ..
                    } => i64::from(*id),
                    MotionExecutionEvent::Completed => -1,
                    _ => panic!("unexpected event"),
                })
                .collect();
            let expected: Vec<_> = step["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|event| {
                    let event = event.as_i64().unwrap();
                    if event == -1 {
                        completed += 1;
                        (completed == 2).then_some(event)
                    } else {
                        Some(event)
                    }
                })
                .collect();
            assert_eq!(actual, expected, "tick={tick} case={case}");
            assert_eq!(
                playback.cursor().segment,
                step["segment"].as_u64().unwrap() as usize
            );
            assert_eq!(
                playback.cursor().frame.to_bits(),
                step["frame_bits"].as_u64().unwrap(),
                "tick={tick}"
            );
        }
    }
}
#[test]
fn rejected_output_capacity_retains_cursor_and_no_early_completion_boundary() {
    let prepared = chain(2.0, 1.0);
    let mut playback = MotionPlayback::new(
        prepared.clone(),
        MotionToken {
            domain: MotionDomain::Casting,
            owner: 1,
            sequence: 1,
        },
    )
    .unwrap();
    let before = playback.cursor();
    let mut output = Vec::new();
    assert!(playback.advance(f64::from(0.2f32), &mut output, 0).is_err());
    assert_eq!(playback.cursor(), before);
    assert!(output.is_empty());
    assert!(
        PreparedMotionChain::prepare(
            prepared.clips().to_vec(),
            2,
            1,
            prepared.physics,
            prepared.motion,
            prepared.speed
        )
        .is_err()
    );
}

#[test]
fn original_manager_completes_zero_link_substate_without_waiting_for_cycle() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    let old = chain(1.0, 1.0);
    for case in fixture["vectors"]["zero_completion"].as_array().unwrap() {
        let count = case["count"].as_u64().unwrap() as usize;
        let prepared = PreparedMotionChain::prepare(
            old.clips()[..count + 1].to_vec(),
            count,
            count,
            old.physics,
            0x4000002b,
            2.0,
        )
        .unwrap();
        let mut playback = MotionPlayback::new(
            Arc::new(prepared),
            MotionToken {
                domain: MotionDomain::Casting,
                owner: 1,
                sequence: 1,
            },
        )
        .unwrap();
        let mut events = Vec::with_capacity(4096);
        playback.advance(0.0, &mut events, 4096).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, MotionExecutionEvent::Completed))
                .count(),
            case["completed"].as_u64().unwrap() as usize
        );
        events.clear();
        playback.advance(0.0, &mut events, 4096).unwrap();
        assert!(events.is_empty());
    }
}

#[test]
fn original_zero_rate_cyclic_posture_is_stationary_after_completion() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_sequence.json")).unwrap();
    let old = chain(1.0, 1.0);
    let mut clip = old.clips()[0].clone();
    clip.framerate = 0.0;
    clip.low = 0;
    clip.high = 0;
    clip.hooks = Vec::new().into();
    let prepared =
        PreparedMotionChain::prepare(vec![clip], 0, 0, old.physics, 0x4000002b, 2.0).unwrap();
    let mut playback = MotionPlayback::new(
        Arc::new(prepared),
        MotionToken {
            domain: MotionDomain::Casting,
            owner: 1,
            sequence: 1,
        },
    )
    .unwrap();
    let mut events = Vec::with_capacity(4096);
    playback.advance(0.0, &mut events, 4096).unwrap();
    assert_eq!(events, vec![MotionExecutionEvent::Completed]);
    events.clear();
    playback
        .advance(f64::from(0.2f32), &mut events, 4096)
        .unwrap();
    assert_eq!(
        playback.cursor().frame,
        fixture["vectors"]["static_cycle"]["frame"]
            .as_f64()
            .unwrap()
    );
    assert_eq!(
        events.len(),
        fixture["vectors"]["static_cycle"]["events"]
            .as_u64()
            .unwrap() as usize
    );
}
