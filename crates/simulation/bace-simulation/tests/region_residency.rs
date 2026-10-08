use bace_simulation::{RegionLifecycleEvent, RegionPhase, RegionResidency, ResidencyError};
fn admit(r: &mut RegionResidency, id: u16, permanent: bool, dungeon: bool) -> u64 {
    let epoch = r.request(id, permanent, 0).unwrap();
    assert_eq!(
        r.take_event(),
        Some(RegionLifecycleEvent::Prepare {
            landblock: id,
            epoch
        })
    );
    r.admit(id, epoch, dungeon, 0).unwrap();
    epoch
}
#[test]
fn heartbeat_strict_timeouts_keepalive_and_generation_fences() {
    let mut r = RegionResidency::new(3).unwrap();
    let epoch = admit(&mut r, 1, false, true);
    admit(&mut r, 2, true, true);
    let keep = admit(&mut r, 3, false, true);
    r.keep_alive(3, keep, 1).unwrap();
    r.advance(750).unwrap();
    assert_eq!(r.state(1).unwrap().phase, RegionPhase::Active);
    r.advance(900).unwrap();
    assert_eq!(
        r.take_event(),
        Some(RegionLifecycleEvent::Dormant {
            landblock: 1,
            epoch
        })
    );
    r.advance(8850).unwrap();
    assert_eq!(r.state(1).unwrap().phase, RegionPhase::Dormant);
    r.advance(9000).unwrap();
    assert_eq!(
        r.take_event(),
        Some(RegionLifecycleEvent::Unload {
            landblock: 1,
            epoch
        })
    );
    assert_eq!(r.request(1, false, 9000), Err(ResidencyError::Busy));
    assert_eq!(r.state(2).unwrap().phase, RegionPhase::Active);
    assert_eq!(r.state(3).unwrap().phase, RegionPhase::Active);
    r.confirm_unloaded(1, epoch).unwrap();
    let next = r.request(1, false, 9000).unwrap();
    assert!(next > epoch);
    assert_eq!(r.admit(1, epoch, true, 9000), Err(ResidencyError::Stale));
}
#[test]
fn outdoor_neighbors_wake_together_and_full_queue_is_atomic() {
    let mut r = RegionResidency::new(2).unwrap();
    admit(&mut r, 0x1234, false, false);
    admit(&mut r, 0x1235, false, false);
    r.advance(900).unwrap();
    assert_eq!(r.activity(0x1234, 901), Err(ResidencyError::Capacity));
    assert_eq!(r.state(0x1234).unwrap().phase, RegionPhase::Dormant);
    r.take_event();
    r.take_event();
    r.activity(0x1234, 901).unwrap();
    assert!(
        r.states()
            .all(|(_, s)| s.phase == RegionPhase::Active && s.last_active_tick == 901)
    );
    assert_eq!(r.advance(u64::MAX), Err(ResidencyError::Invalid));
    assert!(r.states().all(|(_, s)| s.phase == RegionPhase::Active));
}

#[test]
fn shutdown_drains_permanent_keepalive_and_pending_regions_without_losing_epochs() {
    let mut r = RegionResidency::new(3).unwrap();
    let permanent = admit(&mut r, 1, true, true);
    let kept = admit(&mut r, 2, false, true);
    r.keep_alive(2, kept, 1).unwrap();
    let preparing = r.request(3, true, 0).unwrap();
    r.begin_drain();
    assert_eq!(r.request(4, false, 1), Err(ResidencyError::Busy));
    r.advance(1).unwrap();
    assert_eq!(r.state(1).unwrap().phase, RegionPhase::Draining);
    assert_eq!(r.state(2).unwrap().phase, RegionPhase::Draining);
    assert_eq!(r.state(3).unwrap().phase, RegionPhase::Preparing);
    // The already accepted preparation remains first and can complete under
    // its original epoch. A full outbox must not forget the next unload.
    assert_eq!(
        r.take_event(),
        Some(RegionLifecycleEvent::Prepare {
            landblock: 3,
            epoch: preparing
        })
    );
    r.admit(3, preparing, true, 1).unwrap();
    r.advance(2).unwrap();
    assert_eq!(r.state(3).unwrap().phase, RegionPhase::Draining);
    assert_eq!(
        r.confirm_unloaded(1, permanent),
        Err(ResidencyError::Capacity)
    );
    assert!(r.state(1).is_some());
    for (id, epoch) in [(1, permanent), (2, kept), (3, preparing)] {
        assert_eq!(
            r.take_event(),
            Some(RegionLifecycleEvent::Unload {
                landblock: id,
                epoch
            })
        );
    }
    for (id, epoch) in [(1, permanent), (2, kept), (3, preparing)] {
        r.confirm_unloaded(id, epoch).unwrap();
    }
    assert!(
        r.has_state(),
        "unloaded acknowledgments still belong to the host"
    );
    while r.take_event().is_some() {}
    assert!(!r.has_state());
    assert!(r.is_quiescing());
}
