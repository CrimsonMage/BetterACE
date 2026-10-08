use super::*;
#[test]
fn quarter_hour_rounding_and_limits_match_original_ace_clock() {
    let mut count = 0;
    for row in include_str!("../tests/fixtures/generator_clock.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = row.split(',').collect();
        let seconds = f[0].parse().unwrap();
        assert_eq!(
            generator_hour(seconds).unwrap(),
            f[1].parse::<u8>().unwrap(),
            "{row}"
        );
        assert_eq!(generator_is_day(seconds).unwrap(), f[2] == "True", "{row}");
        count += 1;
    }
    assert!(count > 300);
    for invalid in [
        -1.0,
        f64::NAN,
        f64::INFINITY,
        crate::network::PortalClock::MAX_SECONDS + 1.0,
    ] {
        assert!(generator_is_day(invalid).is_err());
    }
}
#[test]
fn utc_pair_preserves_ace_fixed_est_origin_without_local_timezone() {
    let now = std::time::Instant::now();
    let origin = crate::game_bootstrap::GameClock::from_pair(1_485_882_000_000, now).unwrap();
    assert_eq!(origin.portal_origin, 0.0);
    let later = crate::game_bootstrap::GameClock::from_pair(1_485_882_001_500, now).unwrap();
    assert_eq!(later.portal_origin, 1.5);
    assert!(crate::game_bootstrap::GameClock::from_pair(1_485_881_999_999, now).is_err());
}
