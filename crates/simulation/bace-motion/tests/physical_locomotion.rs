use bace_geometry::Vec3;
use bace_motion::*;
fn profile() -> LocomotionProfile {
    let zero = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    LocomotionProfile {
        style: 0x8000003d,
        ready: zero,
        walk: MotionPhysics {
            velocity: Vec3::new(0., 1., 0.),
            ..zero
        },
        run: MotionPhysics {
            velocity: Vec3::new(0., 2., 0.),
            ..zero
        },
        sidestep: MotionPhysics {
            velocity: Vec3::new(1., 0., 0.),
            ..zero
        },
        turn: MotionPhysics {
            omega: Vec3::new(0., 0., 1.),
            ..zero
        },
    }
}
#[test]
fn original_motioninterp_vectors_cover_independent_holdkeys_signed_axes_and_run_rates() {
    let mut count = 0;
    for line in include_str!("fixtures/physical_locomotion.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let row: Vec<_> = line.split(',').collect();
        let n = |i: usize| row[i].parse::<u32>().unwrap();
        let f = |i: usize| row[i].parse::<f32>().unwrap();
        let mut raw = RawLocomotion {
            current_hold: n(1),
            forward: (0x41000003, 0, 1.),
            sidestep: (0, 0, 1.),
            turn: (0, 0, 1.),
        };
        let value = (n(3), n(2), f(4));
        match n(0) {
            0 => raw.forward = value,
            1 => raw.sidestep = value,
            _ => raw.turn = value,
        };
        let drive = profile()
            .interpret_axes(interpret_raw_controls(raw).unwrap(), f(5))
            .unwrap();
        let (actual_command, actual_speed) = match n(0) {
            0 => (drive.forward_motion, drive.forward_rate),
            1 => (0x6500000f, drive.side_rate),
            _ => (0x6500000d, drive.turn_rate),
        };
        assert_eq!(actual_command, n(6));
        assert_eq!(actual_speed.to_bits(), f(7).to_bits(), "{line}");
        count += 1;
    }
    assert_eq!(count, 432);
}
#[test]
fn explicit_axis_override_is_not_replaced_by_current_run_key() {
    let raw = RawLocomotion {
        current_hold: 2,
        forward: (0x45000005, 1, 1.),
        sidestep: (0x65000010, 2, 1.),
        turn: (0x6500000e, 0, 1.),
    };
    let drive = profile()
        .interpret_axes(interpret_raw_controls(raw).unwrap(), 2.5)
        .unwrap();
    assert_eq!(drive.forward_motion, 0x45000005);
    assert_eq!(drive.forward_rate, 1.);
    assert_eq!(drive.side_rate, -3.);
    assert_eq!(drive.turn_rate, -1.5);
    for bad in [f32::NAN, f32::INFINITY, -1.0001, 1.0001] {
        assert!(
            interpret_raw_controls(RawLocomotion {
                forward: (0x45000005, 0, bad),
                ..raw
            })
            .is_err()
        );
    }
    for command in [0x44000007, 0x10000063, 0x80000049, 0x4000001e] {
        assert!(
            interpret_raw_controls(RawLocomotion {
                forward: (command, 0, 1.),
                ..raw
            })
            .is_err()
        );
    }
    assert!(
        interpret_raw_controls(RawLocomotion {
            current_hold: 3,
            ..raw
        })
        .is_err()
    );
}
