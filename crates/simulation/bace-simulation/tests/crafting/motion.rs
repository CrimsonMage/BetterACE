use super::*;
use bace_motion::{
    ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame, SourceMotionState,
    SourceMotionTransition,
};
fn clap() -> Arc<PreparedMotionChain> {
    let ready = SourceMotionState {
        style: 0x8000003d,
        substate: 0x41000003,
        speed: 1.0,
    };
    let clip = ExecutionClip {
        animation: 0x03000001,
        frame_count: 6,
        low: 0,
        high: 5,
        framerate: 30.,
        frames: vec![RootFrame::default(); 6].into(),
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
        PreparedMotionChain::prepare(vec![clip.clone(), clip], 1, 1, physics, 0x1300007e, 1.)
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
fn begin(quote: bool) -> bace_simulation::CraftingCommand {
    bace_simulation::CraftingCommand {
        correlation: 19,
        action: bace_simulation::CraftingCommandKind::BeginUse {
            context: action(1),
            input: Box::new(bace_simulation::TinkerCommandInput {
                context: context(),
                source: craft_item(2),
                target: craft_item(3),
                recipe: Arc::new(recipe()),
            }),
            motion: clap(),
            quote,
            lifetime: 1800,
        },
    }
}
#[test]
fn animation_finishes_before_quote_without_consuming_second_action() {
    let mut k = kernel();
    assert!(matches!(
        k.apply_crafting_command(begin(true)).result,
        Ok(bace_simulation::CraftingResult::Animating {
            command: 0x1300007e,
            ..
        })
    ));
    assert!(k.take_crafting_proposal().is_none());
    for _ in 0..3 {
        k.step().unwrap();
        assert!(k.take_crafting_outcome().is_none());
    }
    let mut quoted = None;
    for _ in 0..30 {
        k.step().unwrap();
        if let Some(result) = k.take_crafting_outcome() {
            quoted = Some(result);
            break;
        }
    }
    assert!(matches!(
        quoted.unwrap().result,
        Ok(bace_simulation::CraftingResult::Quoted(_))
    ));
    assert!(k.take_crafting_proposal().is_none());
    let op = k
        .confirm_tinker(
            action(2),
            &context(),
            &craft_item(2),
            &craft_item(3),
            &recipe(),
        )
        .unwrap();
    assert!(k.take_crafting_proposal().is_some());
    k.reject_crafting(op).unwrap();
}
#[test]
fn animation_completion_waits_for_correlated_output_capacity_and_no_dialog_stages_once() {
    let mut k = kernel();
    assert!(k.apply_crafting_command(begin(false)).result.is_ok());
    for i in 0..16 {
        k.enqueue(bace_simulation::Command::Crafting(
            bace_simulation::CraftingCommand {
                correlation: 100 + i,
                action: bace_simulation::CraftingCommandKind::Cancel {
                    context: action(2 + i as u32),
                },
            },
        ))
        .unwrap();
    }
    for _ in 0..10 {
        k.step().unwrap();
    }
    assert!(k.take_crafting_proposal().is_none());
    for _ in 0..16 {
        assert!(k.take_crafting_outcome().is_some());
    }
    k.step().unwrap();
    let outcome = k.take_crafting_outcome().unwrap();
    assert_eq!(outcome.correlation, 19);
    let Ok(bace_simulation::CraftingResult::Pending(op)) = outcome.result else {
        panic!("{outcome:?}")
    };
    assert_eq!(k.take_crafting_proposal().unwrap().operation, op);
    k.step().unwrap();
    assert!(k.take_crafting_outcome().is_none());
    assert!(k.inventory_item(EntityId(2)).is_some());
    k.reject_crafting(op).unwrap();
}
