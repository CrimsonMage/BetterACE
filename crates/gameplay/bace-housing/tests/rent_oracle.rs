#[test]
fn purchase_aligned_rent_periods_and_apartments_match_unchanged_ace_methods() {
    for row in include_str!("fixtures/rent.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let n: Vec<i64> = row.split(',').map(|v| v.parse().unwrap()).collect();
        assert_eq!(
            bace_housing::rent_window(n[1], n[2], 30 * 86400, n[0] == 1).unwrap(),
            (n[3], n[4]),
            "{row}"
        );
    }
}
