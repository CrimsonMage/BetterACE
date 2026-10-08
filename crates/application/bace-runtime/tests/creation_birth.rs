use bace_runtime::creation_assets::creation_birth_date;
#[test]
fn explicit_utc_birth_date_matches_pinned_original_constructor() {
    for row in include_str!("fixtures/creation_birth.csv")
        .lines()
        .filter(|r| !r.starts_with('#'))
    {
        let (clock, expected) = row.split_once(',').unwrap();
        assert_eq!(
            creation_birth_date(clock.parse().unwrap()).unwrap(),
            expected
        );
    }
    assert!(creation_birth_date(253_402_300_800_000).is_err());
    assert!(creation_birth_date(u64::MAX).is_err());
}
