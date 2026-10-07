use bace_geometry::Vec3;
use bace_runtime::simulation::{SimulationConfig, SimulationInput, SimulationWorker, WorkerError};
use bace_simulation::Command;
use bace_types::{CellId, EntityId};
use std::sync::mpsc::{self, TrySendError};
use std::time::Duration;

fn invalid_command() -> Command {
    Command::ServerTeleport {
        actor: EntityId(u32::MAX),
        cell: CellId(0x0001_0001),
        position: Vec3::new(0.0, 0.0, 1.0),
    }
}

fn refill_until_closed(input: SimulationInput) -> u64 {
    let mut accepted = 0;
    loop {
        match input.try_submit(invalid_command()) {
            Ok(()) => accepted += 1,
            Err(TrySendError::Full(_)) => std::thread::yield_now(),
            Err(TrySendError::Disconnected(_)) => return accepted,
        }
    }
}

#[test]
fn finite_kernel_is_owned_by_a_different_thread() {
    let kernel = bace_simulation::synthetic_scenario(10, 100).unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            tick_limit: Some(100),
            real_time: false,
            ..SimulationConfig::default()
        },
    )
    .unwrap();
    let report = worker.wait().unwrap();
    assert_ne!(report.thread_id, std::thread::current().id());
    assert_eq!(report.ticks, 100);
    assert_eq!(report.rejected_commands, 0);
    assert_eq!(report.discarded_commands, 0);
}

#[test]
fn adapter_sleep_does_not_stop_simulation_and_shutdown_joins_owner() {
    let kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            tick_limit: Some(3),
            ..SimulationConfig::default()
        },
    )
    .unwrap();
    // Represents a blocked caller/adapter; the finite worker still completes.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(worker.wait().unwrap().ticks, 3);
    let kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let worker = SimulationWorker::spawn(kernel, SimulationConfig::default()).unwrap();
    let input = worker.input();
    worker.shutdown().unwrap();
    assert!(matches!(
        input.try_submit(invalid_command()),
        Err(TrySendError::Disconnected(_))
    ));
}

#[test]
fn unbounded_wait_rejects_and_joins_instead_of_losing_the_stop_handle() {
    let kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let worker = SimulationWorker::spawn(kernel, SimulationConfig::default()).unwrap();
    let input = worker.input();
    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || sender.send(worker.wait()).unwrap());
    let result = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("unbounded wait must stop and join");
    assert!(matches!(result, Err(WorkerError::UnboundedWait)));
    waiter.join().unwrap();
    assert!(matches!(
        input.try_submit(invalid_command()),
        Err(TrySendError::Disconnected(_))
    ));
}

#[test]
fn continuous_refill_cannot_starve_finite_ticks_or_escape_exit_accounting() {
    let kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 4,
            tick_limit: Some(8),
            real_time: true,
        },
    )
    .unwrap();
    let input = worker.input();
    let producer = std::thread::spawn(move || refill_until_closed(input));
    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || sender.send(worker.wait()).unwrap());
    let report = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("input flood must not starve ticks")
        .unwrap();
    waiter.join().unwrap();
    let accepted = producer.join().unwrap();
    assert_eq!(report.ticks, 8);
    assert!(accepted > 0);
    assert_eq!(
        accepted,
        report.rejected_commands + report.discarded_commands
    );
    assert!(report.rejected_commands <= 8 * 4);
    assert!(report.discarded_commands <= 4);
}

#[test]
fn shutdown_closes_continuous_producer_before_counting_the_remaining_queue() {
    let kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 4,
            ..SimulationConfig::default()
        },
    )
    .unwrap();
    let input = worker.input();
    // Queue one command before shutdown races with a continuously active sender.
    input.try_submit(invalid_command()).unwrap();
    let producer = std::thread::spawn(move || refill_until_closed(input));
    let report = worker.shutdown().unwrap();
    let accepted = 1 + producer.join().unwrap();
    assert_eq!(
        accepted,
        report.rejected_commands + report.discarded_commands
    );
    assert!(report.discarded_commands <= 4);
}
