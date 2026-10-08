use bace_runtime::simulation::{SimulationConfig, SimulationWorker};
use bace_simulation::{Command, Kernel, NpcServiceAction, NpcServiceCommand};
use bace_types::EntityId;

fn command(correlation: u64) -> Command {
    Command::NpcService(Box::new(NpcServiceCommand {
        correlation,
        action: NpcServiceAction::Checkpoint {
            source: EntityId(999),
        },
    }))
}
#[test]
fn npc_receipt_output_pressure_retains_exact_work_through_worker_recovery() {
    let mut kernel = Kernel::with_gameplay_limits(bace_world::World::default(), 8, 1, 1).unwrap();
    assert!(kernel.try_enqueue(command(0)).is_err());
    for correlation in 1..=3 {
        kernel.enqueue(command(correlation)).unwrap();
    }
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 1,
            tick_limit: Some(2),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert!(exit.failure.is_none());
    let mut outcomes = exit.npc_services.outcomes;
    for _ in 0..4 {
        while let Some(outcome) = exit.kernel.take_npc_service_outcome() {
            outcomes.push(outcome);
        }
        exit.kernel.step().unwrap();
    }
    while let Some(outcome) = exit.kernel.take_npc_service_outcome() {
        outcomes.push(outcome);
    }
    outcomes.sort_by_key(|o| o.correlation);
    assert_eq!(
        outcomes.iter().map(|o| o.correlation).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(outcomes.iter().all(|o| o.result.is_err()));
    assert!(!exit.kernel.has_npc_service_work());
}
