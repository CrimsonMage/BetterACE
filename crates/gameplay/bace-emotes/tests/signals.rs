use bace_emotes::{SignalCylinder, cylinder_distance};
use bace_geometry::Vec3;
#[test]
fn cylinder_distance_matches_original_csharp_boundaries() {
    let mut count = 0;
    for line in include_str!("fixtures/signal_distance.csv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let v: Vec<f64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        let a = SignalCylinder {
            position: Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32),
            radius: v[6] as f32,
            height: v[7] as f32,
        };
        let b = SignalCylinder {
            position: Vec3::new(v[3] as f32, v[4] as f32, v[5] as f32),
            radius: v[8] as f32,
            height: v[9] as f32,
        };
        assert!(
            (cylinder_distance(a, b).unwrap() - v[10]).abs() < 1e-7,
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 8);
}
#[test]
fn malformed_geometry_is_rejected() {
    let a = SignalCylinder {
        position: Vec3::ZERO,
        radius: 1.0,
        height: 2.0,
    };
    for b in [
        SignalCylinder {
            radius: f32::NAN,
            ..a
        },
        SignalCylinder { height: -1.0, ..a },
        SignalCylinder {
            position: Vec3::new(f32::INFINITY, 0.0, 0.0),
            ..a
        },
    ] {
        assert!(cylinder_distance(a, b).is_err());
    }
}

#[test]
fn use_radius_comparison_preserves_signed_csharp_distance() {
    // WorldObject_Use.cs47–55 at the same pin casts the signed oracle result
    // to float, then compares Float54 without abs/clamping/default replacement.
    let a = SignalCylinder {
        position: Vec3::ZERO,
        radius: 0.5,
        height: 2.0,
    };
    let overlap = cylinder_distance(a, a).unwrap() as f32;
    assert_eq!(overlap, -2.236068_f32);
    assert!(overlap <= -1.0);
    assert!(overlap > -3.0);
    let tangent = cylinder_distance(
        a,
        SignalCylinder {
            position: Vec3::new(1., 0., 0.),
            ..a
        },
    )
    .unwrap() as f32;
    assert_eq!(tangent, 0.0);
    assert!(tangent > -f32::EPSILON);
}
