use super::{context, kernel};
use bace_gameplay_api::{CastRejection, CastRequest};
use bace_runtime::simulation::{SimulationConfig, SimulationWorker};
use bace_simulation::Command;

#[test]
fn stalled_cast_consumer_retains_correlated_rejections_and_continues_ticking() {
    let mut kernel = kernel(8, 2, 1);
    for sequence in 1..=8 {
        kernel
            .enqueue(Command::Cast {
                context: context(sequence),
                request: CastRequest::Untargeted { spell: 123 },
            })
            .unwrap();
    }
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 8,
            tick_limit: Some(20),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert_eq!(exit.report.ticks, 20);
    assert!(exit.failure.is_none());
    let mut outcomes = exit.undelivered_cast_outcomes;
    while let Some(value) = exit.kernel.take_cast_outcome() {
        outcomes.push(value);
    }
    assert_eq!(outcomes.len() + exit.kernel.queued_commands(), 8);
    for _ in 0..8 {
        exit.kernel.step().unwrap();
        while let Some(value) = exit.kernel.take_cast_outcome() {
            outcomes.push(value);
        }
    }
    assert_eq!(
        outcomes
            .iter()
            .map(|o| o.context.sequence)
            .collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>()
    );
    assert!(
        outcomes
            .iter()
            .all(|o| o.result == Err(CastRejection::UnknownSpell))
    );
}
