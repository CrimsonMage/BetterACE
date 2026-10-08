use bace_magic::{ProjectileLifeAction as A, SpellProjectileLifetime};
#[test]
fn original_gdle_range_time_collision_and_destroy_boundaries() {
    let mut state = SpellProjectileLifetime::new(0.0).unwrap();
    let mut previous = None;
    let mut explosions = 0;
    let mut destroyed = false;
    let mut rows = 0;
    for line in include_str!("fixtures/projectile_lifetime.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<f64> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let key = (p[0] as u32, (p[1] as f32).to_bits());
        if previous != Some(key) {
            state = SpellProjectileLifetime::new(0.0).unwrap();
            explosions = 0;
            destroyed = false;
            previous = Some(key);
            if matches!(key.0, 1 | 2 | 4) {
                assert_eq!(state.collide(0.0, key.0 == 2).unwrap(), A::Explode);
                explosions += 1;
            }
        }
        match state.tick(p[2], key.0 != 3, p[1] as f32, 75.0).unwrap() {
            A::Explode => explosions += 1,
            A::Destroy => destroyed = true,
            A::None => {}
        }
        assert_eq!(explosions, p[3] as u32, "{line}");
        assert_eq!(destroyed, p[4] != 0.0, "{line}");
        rows += 1;
    }
    assert_eq!(rows, 150);
}
#[test]
fn duplicate_hits_do_not_extend_destroy_deadline_and_bad_time_preserves_state() {
    let mut s = SpellProjectileLifetime::new(0.0).unwrap();
    s.collide(1.0, true).unwrap();
    assert!(s.tick(f64::NAN, true, 0.0, 75.0).is_err());
    assert_eq!(s.collide(1.4, true).unwrap(), A::None);
    assert_eq!(s.tick(1.5, true, 0.0, 75.0).unwrap(), A::Destroy);
}
