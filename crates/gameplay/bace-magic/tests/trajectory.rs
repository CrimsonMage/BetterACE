use bace_geometry::Vec3;
use bace_magic::projectile_velocity;
#[test]
fn compiled_gdle_lead_and_arc_vectors() {
    let mut count = 0;
    for row in include_str!("fixtures/trajectory.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<f32> = row.split(',').map(|v| v.parse().unwrap()).collect();
        let got = projectile_velocity(
            Vec3::new(f[0], f[1], f[2]),
            Vec3::new(f[3], f[4], f[5]),
            f[6],
            f[7] != 0.0,
            if f[8] != 0.0 { 9.8 } else { 0.0 },
        )
        .unwrap();
        for (actual, expected) in [got.x, got.y, got.z].into_iter().zip(&f[9..12]) {
            assert!((actual - expected).abs() < 0.00001, "{row}: {got:?}");
        }
        count += 1;
    }
    assert_eq!(count, 64);
}
#[test]
fn degenerate_tracking_falls_back_to_finite_direct_launch() {
    let got = projectile_velocity(
        Vec3::new(0.0, 10.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        1.0,
        true,
        0.0,
    )
    .unwrap();
    assert_eq!(got, Vec3::new(0.0, 1.0, 0.0));
    assert!(projectile_velocity(Vec3::ZERO, Vec3::ZERO, 10.0, true, 9.8).is_err());
}
