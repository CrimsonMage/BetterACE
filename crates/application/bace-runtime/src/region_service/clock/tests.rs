use super::*;
#[test]
fn full_queue_retains_day_and_only_exact_success_acknowledges_delivery() {
    let mut clock = ClockDelivery::default();
    clock.update(true);
    clock.flush(|_| false);
    assert_eq!(clock.pending, Some(true));
    assert!(clock.applied.is_none());
    clock.flush(|_| true);
    assert_eq!(clock.inflight, Some(true));
    clock.update(false);
    assert!(clock.pending.is_none());
    assert!(
        clock
            .accept(GeneratorCommandOutcome {
                admission: None,
                request: None,
                correlation: 5,
                result: Ok(())
            })
            .is_err()
    );
    assert!(clock.applied.is_none());
    assert!(
        clock
            .accept(GeneratorCommandOutcome {
                admission: None,
                request: None,
                correlation: CORRELATION,
                result: Ok(())
            })
            .is_ok()
    );
    clock.update(false);
    assert_eq!(clock.pending, Some(false));
}
#[test]
fn rejected_clock_transition_is_observable_and_blocks_shutdown_until_explicit_retry() {
    let mut clock = ClockDelivery::default();
    clock.update(false);
    clock.flush(|_| true);
    assert!(
        clock
            .accept(GeneratorCommandOutcome {
                admission: None,
                request: None,
                correlation: CORRELATION,
                result: Err(bace_simulation::GeneratorServiceError::Busy)
            })
            .is_ok()
    );
    assert!(clock.failure().is_some());
    assert!(clock.has_pending());
    clock.update(true);
    assert!(clock.pending.is_none());
    clock.retry();
    clock.update(true);
    assert_eq!(clock.pending, Some(true));
}
