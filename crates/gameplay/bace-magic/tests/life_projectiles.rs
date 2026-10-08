use bace_magic::life_projectile_drain;
#[test]
fn independent_gdle_source_drain_vectors() {
    let mut count = 0;
    for row in include_str!("fixtures/life.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = row.split(',').collect();
        let current: u32 = f[1].parse().unwrap();
        let drain = life_projectile_drain(current, f[2].parse().unwrap(), f[0] == "0").unwrap();
        assert_eq!(drain, f[3].parse::<u32>().unwrap(), "{row}");
        assert_eq!(current - drain, f[4].parse::<u32>().unwrap(), "{row}");
        count += 1;
    }
    assert_eq!(count, 60);
}
