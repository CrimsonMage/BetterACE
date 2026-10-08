use super::*;
use bace_dat::{AnimationSegment, MotionData};
fn table(layout: u32, low: i32, high: i32, framerate: f32, count: usize) -> MotionTable {
    let mut table = MotionTable {
        id: 0x09000001,
        default_style: 0x8000003d,
        style_defaults: [(0x8000003d, 0x41000003)].into(),
        cycles: BTreeMap::new(),
        modifiers: BTreeMap::new(),
        links: BTreeMap::new(),
    };
    let key = 0x003d0003;
    if layout != 0 {
        table.links.insert(key, BTreeMap::new());
    }
    if layout == 2 {
        table.links.insert(0x003d0000, BTreeMap::new());
    }
    if layout >= 2 {
        table
            .links
            .get_mut(&(if layout == 2 { 0x003d0000 } else { key }))
            .unwrap()
            .insert(
                RecallKind::Lifestone.motion(),
                MotionData {
                    bitfield: 0,
                    flags: 0,
                    velocity: None,
                    omega: None,
                    animations: (0..count)
                        .map(|i| AnimationSegment {
                            animation_id: (i % 2 + 1) as u32,
                            low_frame: low,
                            high_frame: high,
                            framerate,
                        })
                        .collect(),
                },
            );
    }
    table
}
#[test]
fn source_default_link_fallback_and_f32_timing_match_original_csharp() {
    let mut count = 0;
    for line in include_str!("../../../tests/fixtures/recall_timing.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let parts: Vec<_> = line.split(',').collect();
        let layout = parts[0].parse().unwrap();
        let source = table(
            layout,
            parts[1].parse().unwrap(),
            parts[2].parse().unwrap(),
            parts[3].parse().unwrap(),
            parts[4].parse().unwrap(),
        );
        let seconds = source_animation_length(&source, RecallKind::Lifestone.motion(), |id| {
            Some(if id == 1 { 37 } else { 53 })
        })
        .unwrap();
        assert_eq!(
            seconds.to_bits(),
            parts[5].parse::<u32>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 288);
}
#[test]
fn invalid_animation_content_fails_before_motion_admission() {
    for (low, high, rate) in [
        (-1, 37, 30.),
        (20, 13, 30.),
        (0, -2, 30.),
        (0, 37, 0.),
        (0, 37, f32::NAN),
        (0, 37, f32::INFINITY),
    ] {
        assert!(
            source_animation_length(
                &table(3, low, high, rate, 1),
                RecallKind::Lifestone.motion(),
                |_| Some(37)
            )
            .is_err()
        );
    }
    assert!(
        source_animation_length(
            &table(3, 0, -1, 30., 1),
            RecallKind::Lifestone.motion(),
            |_| None
        )
        .is_err()
    );
    let valid = source_animation_length(
        &table(3, 0, -1, -30., 1),
        RecallKind::Lifestone.motion(),
        |_| Some(37),
    )
    .unwrap();
    assert_eq!(valid, 37. / 30.);
    assert_eq!(
        bace_interactions::recall_delay_ticks(RecallKind::Marketplace, 18.4).unwrap(),
        420
    );
}
