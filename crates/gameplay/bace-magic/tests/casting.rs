use bace_geometry::Vec3;
use bace_magic::{
    CastDriver, CastError, CastGesture, CastObservation, CastPreparation, CastSignal, CastStage,
};
fn observed() -> CastObservation {
    CastObservation {
        cell: 1,
        position: Vec3::ZERO,
        alive: true,
        in_portal: false,
        peace_mode: false,
        heading_to_target: None,
        target_valid: true,
        target_in_range: true,
        geometry_clear: true,
        turning_to_target: false,
        manual_turning: false,
    }
}
fn prepared(id: u64) -> CastPreparation {
    CastPreparation {
        id,
        spell: 100,
        target: Some(2),
        gestures: vec![CastGesture {
            motion: 0x13000132,
            minimum_seconds: 0.1,
        }],
        uses_mana: true,
        player: true,
        fast_resistable_pk_spell: false,
    }
}
#[test]
fn slide_distance_boundary_and_release_do_not_fabricate_completion() {
    for distance in [5.999, 6.0] {
        let mut driver = CastDriver::default();
        let CastSignal::Gesture {
            sequence, motion, ..
        } = driver.begin(prepared(1), 0.0, observed()).unwrap()
        else {
            panic!()
        };
        let mut obs = observed();
        obs.position = Vec3::new(distance, 0.0, 0.0);
        let signal = driver
            .motion_done(1, sequence, motion, true, 0.2, obs)
            .unwrap();
        if distance < 6.0 {
            assert!(matches!(signal, CastSignal::Release { cast: 1, .. }));
            assert_eq!(driver.stage(), CastStage::Release);
            assert_eq!(driver.update(1.0, obs).unwrap(), CastSignal::Waiting);
            assert_eq!(
                driver.resolve_release(1, 1.0, Ok(())).unwrap(),
                CastSignal::Finished {
                    cast: 1,
                    error: None
                }
            );
        } else {
            assert!(matches!(signal, CastSignal::Release { cast: 1, .. }));
            assert!(driver.movement_disrupted(obs));
            assert!(matches!(
                driver.movement_fizzle_at_release(1, 0.2).unwrap(),
                CastSignal::MovementFizzle {
                    cast: 1,
                    mana_cost: 5,
                    ..
                }
            ));
            assert!(!driver.active());
        }
    }
}
#[test]
fn gdle_manual_turn_release_uses_45_degrees_but_waits_for_active_server_turn() {
    let mut driver = CastDriver::default();
    let mut obs = observed();
    obs.heading_to_target = Some(90.0);
    assert!(matches!(
        driver.begin(prepared(1), 0.0, obs).unwrap(),
        CastSignal::Turn {
            maximum_angle_degrees: 45.0,
            ..
        }
    ));
    obs.heading_to_target = Some(45.0);
    obs.turning_to_target = true;
    assert_eq!(driver.update(0.1, obs).unwrap(), CastSignal::Waiting);
    obs.turning_to_target = false;
    obs.manual_turning = true;
    assert!(matches!(
        driver.update(0.2, obs).unwrap(),
        CastSignal::Gesture {
            speed: 2.0,
            stop_movement: true,
            ..
        }
    ));
}
#[test]
fn source_hold_timeout_one_active_cast_and_stale_completion_fences() {
    let mut driver = CastDriver::default();
    let mut obs = observed();
    obs.heading_to_target = Some(90.0);
    obs.manual_turning = true;
    driver.begin(prepared(7), 0.0, obs).unwrap();
    assert_eq!(driver.begin(prepared(8), 0.1, obs), Err(CastError::Busy));
    assert_eq!(driver.update(9998.0, obs).unwrap(), CastSignal::Waiting);
    assert_eq!(
        driver.update(9999.0, obs).unwrap(),
        CastSignal::Finished {
            cast: 7,
            error: Some(CastError::TimedOut)
        }
    );
    let CastSignal::Gesture {
        motion, sequence, ..
    } = driver.begin(prepared(8), 10000.0, observed()).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        driver.motion_done(7, sequence, motion, true, 10000.2, observed()),
        Err(CastError::StaleCompletion)
    );
    assert!(driver.active());
    assert_eq!(
        driver
            .motion_done(8, sequence, motion, true, 10004.0, observed())
            .unwrap(),
        CastSignal::Finished {
            cast: 8,
            error: Some(CastError::TimedOut)
        }
    );
}
#[test]
fn invalid_observations_cannot_mutate_active_cast_and_repeated_gestures_keep_indices() {
    let mut p = prepared(1);
    p.gestures.push(p.gestures[0]);
    let mut driver = CastDriver::default();
    let CastSignal::Gesture {
        motion,
        sequence,
        index: 0,
        ..
    } = driver.begin(p, 0.0, observed()).unwrap()
    else {
        panic!()
    };
    let mut bad = observed();
    bad.position.x = f32::NAN;
    assert_eq!(driver.update(0.1, bad), Err(CastError::InvalidObservation));
    assert_eq!(driver.stage(), CastStage::Gesture);
    assert!(matches!(
        driver
            .motion_done(1, sequence, motion, true, 0.2, observed())
            .unwrap(),
        CastSignal::Gesture { index: 1, .. }
    ));
}
#[test]
fn pinned_gdle_cast_control_oracle() {
    for line in include_str!("fixtures/gdle_cast.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let mut driver = CastDriver::default();
        let mut obs = observed();
        match p[0] {
            "begin" => {
                obs.heading_to_target = Some(p[1].parse().unwrap());
                let mut request = prepared(1);
                if p[2] == "0" {
                    request.gestures.clear();
                }
                let signal = driver.begin(request, 10.0, obs).unwrap();
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Turn { .. })),
                    p[3].parse::<u32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Gesture { .. })),
                    p[4].parse::<u32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Release { .. })),
                    p[5].parse::<u32>().unwrap(),
                    "{line}"
                );
            }
            "update" => {
                obs.heading_to_target = Some(90.0);
                driver.begin(prepared(1), 10.0, obs).unwrap();
                obs.heading_to_target = Some(p[1].parse().unwrap());
                obs.turning_to_target = p[2] == "1";
                obs.manual_turning = p[3] == "1";
                let signal = driver.update(11.0, obs).unwrap();
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Turn { .. })),
                    p[4].parse::<u32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Gesture { .. })),
                    p[5].parse::<u32>().unwrap(),
                    "{line}"
                );
            }
            "distance" => {
                obs.heading_to_target = Some(90.0);
                driver.begin(prepared(1), 10.0, obs).unwrap();
                driver.update(10.1, obs).unwrap();
                obs.manual_turning = true;
                obs.position = Vec3::new(p[1].parse().unwrap(), 0.0, 0.0);
                let signal = driver.update(12.0, obs).unwrap();
                assert_eq!(
                    u32::from(driver.active()),
                    p[2].parse::<u32>().unwrap(),
                    "{line}"
                );
                let mut mana = 100u32;
                let fizzled = if let CastSignal::MovementFizzle {
                    mana_cost,
                    intensity,
                    ..
                } = signal
                {
                    mana = mana.saturating_sub(mana_cost);
                    assert_eq!(intensity.to_bits(), 0x3f0af0a2);
                    true
                } else {
                    false
                };
                assert_eq!(mana, p[3].parse::<u32>().unwrap(), "{line}");
                assert_eq!(u32::from(fizzled), p[4].parse::<u32>().unwrap(), "{line}");
            }
            "callback_mode" => {
                let signal = driver.begin(prepared(1), 10.0, obs).unwrap();
                let CastSignal::Gesture {
                    sequence, motion, ..
                } = signal
                else {
                    panic!("expected action")
                };
                obs.peace_mode = p[1] == "1";
                let signal = driver
                    .motion_done(1, sequence, motion, true, 11.0, obs)
                    .unwrap();
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Release { .. })),
                    p[2].parse::<u32>().unwrap()
                );
                driver.resolve_release(1, 11.0, Ok(())).unwrap();
                assert_eq!(driver.update(11.0, obs).unwrap(), CastSignal::Waiting);
                assert_eq!(p[3], "0");
                assert_eq!(p[4], "100");
                assert_eq!(driver.active(), p[5] == "1");
            }
            "mode" => {
                let mut request = prepared(1);
                request.player = p[1] == "1";
                obs.peace_mode = p[2].parse::<u32>().unwrap() == 0x8000003d;
                // GDLE has no magic-style admission guard; peace is handled on
                // the next Update, not by fabricating a rejected begin.
                driver.begin(request, 10.0, obs).unwrap();
                let signal = driver.update(11.0, obs).unwrap();
                assert_eq!(driver.active(), p[4] == "1", "{line}");
                let mut mana = p[3].parse::<u32>().unwrap();
                let fizzled = if let CastSignal::PeaceFizzle {
                    mana_cost,
                    intensity,
                    ..
                } = signal
                {
                    mana = mana.saturating_sub(mana_cost);
                    assert_eq!(intensity.to_bits(), p[9].parse::<u32>().unwrap());
                    assert_eq!(
                        p[8], "0",
                        "source LaunchSpellEffect(TRUE) returns WERROR_NONE"
                    );
                    true
                } else {
                    false
                };
                assert_eq!(mana, p[5].parse::<u32>().unwrap(), "{line}");
                assert_eq!(u32::from(fizzled), p[6].parse::<u32>().unwrap(), "{line}");
                assert_eq!(p[7], "0", "peace fizzle never executes the spell");
            }
            "instant" => {
                let mut request = prepared(1);
                request.player = false;
                request.uses_mana = false;
                request.gestures.clear();
                obs.alive = p[1] == "0";
                let signal = driver.begin(request, 10.0, obs).unwrap();
                assert_eq!(
                    u32::from(matches!(signal, CastSignal::Release { .. })),
                    p[5].parse::<u32>().unwrap()
                );
                assert_eq!(
                    p[4], p[6],
                    "instant native method leaves source mana unchanged"
                );
                assert_eq!(
                    p[2], p[7],
                    "native instant call preserves prior casting flag"
                );
                assert_eq!(p[8], "0");
            }
            _ => panic!("unexpected oracle row"),
        }
    }
}
#[test]
fn instant_existing_source_ignores_player_liveness_but_keeps_target_checks() {
    let mut request = prepared(1);
    request.player = false;
    request.uses_mana = false;
    request.gestures.clear();
    let mut obs = observed();
    obs.alive = false;
    obs.in_portal = true;
    obs.heading_to_target = Some(90.);
    let mut driver = CastDriver::default();
    assert!(matches!(
        driver.begin(request.clone(), 0., obs).unwrap(),
        CastSignal::Release { .. }
    ));
    assert!(matches!(
        driver.resolve_release(1, 0., Ok(())).unwrap(),
        CastSignal::Finished { error: None, .. }
    ));
    obs.target_in_range = false;
    assert_eq!(
        CastDriver::default().begin(request, 0., obs),
        Err(CastError::OutOfRange)
    );
    let mut player = prepared(2);
    player.gestures.clear();
    assert_eq!(
        CastDriver::default().begin(player, 0., obs),
        Err(CastError::Dead)
    );
}
