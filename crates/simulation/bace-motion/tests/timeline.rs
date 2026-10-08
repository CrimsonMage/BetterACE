use bace_geometry::Vec3;
use bace_motion::*;
#[test]
fn partial_reverse_ranges_and_hook_backpressure_preserve_once_only_delivery() {
    let hook = |frame, direction| MotionHook {
        frame,
        direction,
        kind: 3,
        payload: MotionHookPayload::Attack {
            part: 1,
            left: [1., 0.],
            right: [1., 0.],
            radius: 1.,
            height: 1.,
        },
    };
    let motion = PreparedMotion::prepare(
        &[MotionSegment {
            animation: 3,
            frame_count: 10,
            low_frame: 2,
            high_frame: 8,
            frames_per_second: -2.,
            hooks: vec![hook(7, -1), hook(5, 0), hook(3, 1), hook(1, 0)],
        }],
        Vec3::ZERO,
        Vec3::ZERO,
    )
    .unwrap();
    assert_eq!(motion.duration_seconds(), 3.0);
    assert_eq!(motion.hooks().len(), 2);
    assert_eq!(motion.hooks()[0].seconds, 0.0);
    assert_eq!(motion.hooks()[1].seconds, 1.0);
    let mut cursor = motion.cursor();
    assert!(cursor.advance(&motion, 1.0, 1).is_err());
    assert_eq!(cursor.advance(&motion, 1.0, 2).unwrap().len(), 2);
    assert!(cursor.advance(&motion, 2.0, 0).unwrap().is_empty());
    assert!(cursor.finished(&motion));
}
