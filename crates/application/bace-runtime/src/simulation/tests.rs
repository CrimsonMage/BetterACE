use super::*;
use bace_geometry::Vec3;
use bace_types::{CellId, EntityId};

fn command() -> Command {
    Command::ServerTeleport {
        actor: EntityId(u32::MAX),
        cell: CellId(0x0001_0001),
        position: Vec3::new(0.0, 0.0, 1.0),
    }
}

fn channel(capacity: usize) -> (SimulationInput, SimulationInbox) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let admission = Arc::new(Mutex::new(true));
    (
        SimulationInput {
            sender,
            admission: Arc::clone(&admission),
        },
        SimulationInbox {
            receiver,
            admission,
        },
    )
}

#[test]
fn full_returns_command_ownership_without_overwriting_queued_work() {
    let (input, inbox) = channel(1);
    input.try_submit(command()).unwrap();
    let Err(mpsc::TrySendError::Full(returned)) = input.try_submit(command()) else {
        panic!("a full bounded queue must return its command");
    };
    assert!(matches!(
        returned,
        Command::ServerTeleport {
            actor: EntityId(u32::MAX),
            ..
        }
    ));
    assert_eq!(inbox.close_and_discard(), 1);
    assert!(matches!(
        input.try_submit(returned),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
}

#[test]
fn stopped_worker_counts_every_queued_command_and_closes_admission() {
    let (input, inbox) = channel(3);
    for _ in 0..3 {
        input.try_submit(command()).unwrap();
    }
    let report = drive(
        bace_simulation::synthetic_scenario(1, 0).unwrap(),
        inbox,
        Arc::new(AtomicBool::new(true)),
        SimulationConfig {
            command_capacity: 3,
            ..SimulationConfig::default()
        },
    )
    .unwrap();
    assert_eq!(report.ticks, 0);
    assert_eq!(report.rejected_commands, 0);
    assert_eq!(report.discarded_commands, 3);
    assert_eq!(report.p99_upper_bound, None);
    assert!(matches!(
        input.try_submit(command()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
}

#[test]
fn finite_worker_rejects_processed_invalid_command_without_counting_it_discarded() {
    let (input, inbox) = channel(1);
    input.try_submit(command()).unwrap();
    let report = drive(
        bace_simulation::synthetic_scenario(1, 0).unwrap(),
        inbox,
        Arc::new(AtomicBool::new(false)),
        SimulationConfig {
            command_capacity: 1,
            tick_limit: Some(1),
            real_time: false,
        },
    )
    .unwrap();
    assert_eq!(report.ticks, 1);
    assert_eq!(report.rejected_commands, 1);
    assert_eq!(report.discarded_commands, 0);
    assert!(matches!(
        input.try_submit(command()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
}

#[test]
fn inbox_guard_closes_admission_during_error_return() {
    fn fail(inbox: SimulationInbox) -> Result<(), SimulationError> {
        let _guard = inbox;
        Err(SimulationError::QueueFull)?;
        Ok(())
    }
    let (input, inbox) = channel(1);
    input.try_submit(command()).unwrap();
    assert!(fail(inbox).is_err());
    assert!(!*input.admission.lock().unwrap());
    assert!(matches!(
        input.try_submit(command()),
        Err(mpsc::TrySendError::Disconnected(_))
    ));
}

#[test]
fn preloaded_progression_requests_require_recovery_even_without_a_character() {
    use bace_gameplay_api::*;
    let mut kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    kernel
        .enqueue(Command::RaiseProgression {
            context: ActionContext {
                session: SessionId(1),
                account: bace_types::AccountId(1),
                actor: EntityId(1),
                sequence: 1,
            },
            request: RaiseProgression {
                target: ProgressionTarget::Attribute(AttributeId::Strength),
                amount: 0,
            },
        })
        .unwrap();
    let (_input, inbox) = channel(1);
    let (output, _receive) = mpsc::sync_channel(1);
    let recovered = drive_owned(
        kernel,
        inbox,
        Arc::new(AtomicBool::new(true)),
        SimulationConfig::default(),
        output,
    );
    assert_eq!(recovered.report.ticks, 0);
    assert!(!recovered.kernel.has_characters());
    assert!(recovered.kernel.has_queued_progression());
    assert!(recovered.requires_recovery());
}
