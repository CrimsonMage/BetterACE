// Synthetic authority regressions. Source timer vectors and real-DAT chains are
// independently qualified in bace-runtime; these tests make no client claim.
use super::*;
use bace_motion::{
    ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame, SourceMotionState,
    SourceMotionTransition,
};

fn motion(kind: RecallKind, frames: u32) -> Arc<PreparedMotionChain> {
    motion_command(kind.motion(), frames)
}

pub(super) fn binding_motion(frames: u32) -> Arc<PreparedMotionChain> {
    motion_command(0x1000_0057, frames)
}

fn motion_command(command: u32, frames: u32) -> Arc<PreparedMotionChain> {
    let ready = SourceMotionState {
        style: 0x8000003d,
        substate: 0x41000003,
        speed: 1.,
    };
    let clip = ExecutionClip {
        animation: 0x03000001,
        frame_count: frames,
        low: 0,
        high: -1,
        framerate: 30.,
        frames: vec![RootFrame::default(); frames as usize].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![clip.clone()], 0, 0, physics, ready.substate, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: true,
            })
            .unwrap(),
    );
    Arc::new(
        PreparedMotionChain::prepare(vec![clip.clone(), clip], 1, 1, physics, command, 1.)
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
fn start(
    k: &mut Kernel,
    sequence: u32,
    kind: RecallKind,
    revision: u64,
    chain: Arc<PreparedMotionChain>,
) {
    k.apply_recall_command(RecallCommand::StartPrepared {
        context: context(sequence),
        kind,
        before_revision: revision,
        animation_seconds: chain.nominal_duration_seconds(),
        style: None,
        motion: chain,
    })
    .unwrap();
}
#[test]
fn stale_or_wrong_motion_does_not_debit_mana_or_reserve_recall_ordinal() {
    let mut k = prepared();
    let revision = k.character(EntityId(1)).unwrap().revision();
    let mana = k
        .world()
        .vital(EntityId(1), EntityVital::Mana)
        .unwrap()
        .current;
    start(
        &mut k,
        1,
        RecallKind::Lifestone,
        revision + 1,
        motion(RecallKind::Lifestone, 6),
    );
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Retry { .. })
    ));
    start(
        &mut k,
        1,
        RecallKind::Lifestone,
        revision,
        motion(RecallKind::Marketplace, 6),
    );
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Rejected {
            error: RecallError::MissingAssets,
            ..
        })
    ));
    assert!(!k.recall_busy(EntityId(1)));
    assert!(k.world().source_motion_token(EntityId(1)).is_none());
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        mana
    );
    start(
        &mut k,
        3,
        RecallKind::Lifestone,
        revision,
        motion(RecallKind::Lifestone, 6),
    );
    assert!(
        matches!(k.take_recall_event(), Some(RecallEvent::Started { mana_after: Some(value), .. }) if value == mana/2)
    );
    let token = k.world().source_motion_token(EntityId(1)).unwrap();
    assert_eq!(token.domain, bace_motion::MotionDomain::Recall);
    assert_eq!(token.owner, 1);
    k.apply_recall_command(RecallCommand::Cancel { actor: EntityId(1) })
        .unwrap();
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Cancelled { .. })
    ));
    for _ in 0..30 {
        step(&mut k);
    }
    assert!(k.take_portal_proposal().is_none());
    assert!(!k.recall_busy(EntityId(1)));
}
#[test]
fn marketplace_stages_at_fourteen_seconds_while_longer_animation_is_unfinished() {
    let mut k = prepared();
    let revision = k.character(EntityId(1)).unwrap().revision();
    start(
        &mut k,
        1,
        RecallKind::Marketplace,
        revision,
        motion(RecallKind::Marketplace, 600),
    );
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Started {
            until_tick: 420,
            ..
        })
    ));
    // Kernel::step executes the current numbered phase before advancing ticks.
    while k.ticks() < 420 {
        step(&mut k);
        assert!(k.take_portal_proposal().is_none());
    }
    assert!(k.world().motion_busy(EntityId(1)));
    step(&mut k);
    assert!(k.take_portal_proposal().is_some());
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Staged {
            kind: RecallKind::Marketplace,
            ..
        })
    ));
    assert!(!k.recall_busy(EntityId(1)));
}

#[test]
fn combat_recall_admits_style_and_action_atomically_without_delaying_the_timer() {
    let mut k = prepared_in_mode(8);
    let revision = k.character(EntityId(1)).unwrap().revision();
    let action = motion(RecallKind::Marketplace, 600);
    let ready = action.source_transition().unwrap().before;
    let clip = ExecutionClip {
        animation: 0x03000002,
        frame_count: 30,
        low: 0,
        high: -1,
        framerate: 30.,
        frames: vec![RootFrame::default(); 30].into(),
        hooks: vec![].into(),
    };
    let style = Arc::new(
        PreparedMotionChain::prepare(
            vec![clip.clone(), clip],
            1,
            1,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            0x8000003d,
            1.,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: SourceMotionState {
                style: 0x8000003e,
                ..ready
            },
            after: ready,
            continues_cycle: false,
        })
        .unwrap(),
    );
    k.apply_recall_command(RecallCommand::StartPrepared {
        context: context(1),
        kind: RecallKind::Marketplace,
        before_revision: revision,
        animation_seconds: 20.,
        style: Some(style),
        motion: action,
    })
    .unwrap();
    assert!(matches!(
        k.take_recall_event(),
        Some(RecallEvent::Started {
            until_tick: 420,
            combat_mode_changed: true,
            ..
        })
    ));
    assert_eq!(k.world().combatant(EntityId(1)).unwrap().mode(), 1);
    while k.ticks() < 420 {
        step(&mut k);
        assert!(k.take_portal_proposal().is_none());
    }
    step(&mut k);
    assert!(k.take_portal_proposal().is_some());
}
