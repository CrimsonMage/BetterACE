use bace_geometry::Vec3;
use bace_motion::{
    ExecutionClip, MotionClipRate, MotionPhysics, MotionRate, MotionRateModel, PreparedMotionChain,
    RootFrame, SourceMotionState, SourceMotionTransition,
};
use std::sync::Arc;
#[test]
fn equal_old_rates_do_not_confuse_action_ready_and_current_link_roles() {
    let clip = ExecutionClip {
        animation: 1,
        frame_count: 2,
        low: 0,
        high: 1,
        framerate: 30.0,
        frames: vec![RootFrame::default(); 2].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::new(0.0, 1.0, 0.0),
        omega: Vec3::ZERO,
    };
    let source = SourceMotionState {
        style: 0x80000040,
        substate: 0x41000003,
        speed: 1.0,
    };
    let chain = PreparedMotionChain::prepare(
        vec![clip.clone(), clip.clone(), clip.clone(), clip],
        3,
        3,
        physics,
        0x10000063,
        1.0,
    )
    .unwrap()
    .with_source_transition(SourceMotionTransition {
        before: source,
        after: source,
        continues_cycle: false,
    })
    .unwrap()
    .with_rate_model(MotionRateModel {
        clears_modifiers: false,
        clips: [
            MotionRate::Unit,
            MotionRate::Action,
            MotionRate::Current,
            MotionRate::Current,
        ]
        .map(|role| MotionClipRate { base: 30.0, role })
        .to_vec(),
        cycle_rate: MotionRate::Current,
        cycle_physics: physics,
        modifiers: vec![],
        scale: 1.0,
    })
    .unwrap();
    let faster = chain.retime_action(2.0).unwrap();
    assert_eq!(
        faster
            .clips()
            .iter()
            .map(|c| c.framerate)
            .collect::<Vec<_>>(),
        vec![30.0, 60.0, 30.0, 30.0]
    );
    assert_eq!(faster.physics, chain.physics);
    assert_eq!(faster.source_transition(), chain.source_transition());
    for (a, b) in chain.clips().iter().zip(faster.clips()) {
        assert!(Arc::ptr_eq(&a.frames, &b.frames));
        assert!(Arc::ptr_eq(&a.hooks, &b.hooks));
    }
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, 20.1] {
        assert!(chain.retime_action(bad).is_err());
    }
}
