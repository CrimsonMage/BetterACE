use bace_combat::physical::{
    MissileLifetime, MissileLifetimeEvent::*, charge_seconds, missile_settings,
};

#[test]
fn independent_client_and_gdle_timing_vectors() {
    let mut rows = 0;
    for line in include_str!("fixtures/physical_timing.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let values: Vec<_> = line.split(',').collect();
        let dual = values[1] == "1";
        let power: f64 = values[2].parse().unwrap();
        if values[0] == "client" {
            let actual: f64 = values[3].parse().unwrap();
            let duration = charge_seconds(1.0, dual).unwrap();
            assert!(
                (actual - (power / duration).clamp(0.0, 1.0)).abs() < 1e-12,
                "{line}"
            );
        } else {
            let queued = values[3] == "1";
            let charge: f64 = values[4].parse().unwrap();
            let selected: f32 = values[5].parse().unwrap();
            assert_eq!(selected, if queued { 0.75 } else { power as f32 });
            // GDLE repeats multiply f32 power * 0.8f; selected client scaling
            // uses double 0.8. Record that precision distinction explicitly.
            assert!(
                (charge - 20.0 - charge_seconds(selected, dual).unwrap()).abs() < 5e-8,
                "{line}"
            );
            assert_eq!(&values[6..], &["1", "1", "1"]);
        }
        rows += 1;
    }
    assert_eq!(rows, 32);
}

#[test]
fn retail_dual_charge_and_source_options_boundaries() {
    assert_eq!(charge_seconds(1.0, true).unwrap(), 0.8);
    assert_eq!(charge_seconds(0.5, true).unwrap(), 0.4);
    assert_eq!(charge_seconds(1.0, false).unwrap(), 1.0);
    for power in [
        f32::NAN,
        f32::INFINITY,
        -f32::from_bits(1),
        f32::from_bits(1.0f32.to_bits() + 1),
    ] {
        assert!(charge_seconds(power, true).is_err());
    }
    assert_eq!(missile_settings(20.0, true, 0).unwrap(), (18.0, false));
    assert_eq!(missile_settings(20.0, true, 0x18000).unwrap(), (25.0, true));
    assert_eq!(
        missile_settings(20.0, false, 0x18000).unwrap(),
        (18.0, false)
    );
}

#[test]
fn source_destroy_effect_precedes_physical_retirement_even_after_a_stall() {
    let mut flight = MissileLifetime::launch(10.0).unwrap();
    assert_eq!(flight.poll(14.999).unwrap(), Waiting);
    assert_eq!(flight.poll(15.0).unwrap(), DestroyEffect);
    assert_eq!(flight.poll(16.999).unwrap(), Waiting);
    assert_eq!(flight.poll(17.0).unwrap(), Remove);
    let mut delayed = MissileLifetime::launch(10.0).unwrap();
    assert_eq!(delayed.poll(100.0).unwrap(), DestroyEffect);
    assert_eq!(delayed.poll(100.0).unwrap(), Remove);
}

#[test]
fn environment_hit_retains_resting_ammo_for_one_second_then_destroy_delay() {
    let mut flight = MissileLifetime::launch(10.0).unwrap();
    flight.environment(10.5).unwrap();
    assert!(flight.stopped());
    assert_eq!(flight.poll(11.499).unwrap(), Waiting);
    assert_eq!(flight.poll(11.5).unwrap(), DestroyEffect);
    assert_eq!(flight.poll(13.499).unwrap(), Waiting);
    assert_eq!(flight.poll(13.5).unwrap(), Remove);
    let before = flight;
    assert!(flight.environment(f64::NAN).is_err());
    assert_eq!(flight, before);
}
