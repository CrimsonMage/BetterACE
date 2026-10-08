use bace_config::WorldConfig;
#[test]
fn lists_match_compiled_original_ace_fields() {
    let config = WorldConfig::default();
    for (tag, mut actual) in [
        ("L", config.zones.no_relog),
        ("D", config.zones.no_death_item_drop),
        ("X", config.zones.no_kill_experience),
    ] {
        let mut expected: Vec<u16> = include_str!("fixtures/ace_world_zones.tsv")
            .lines()
            .filter_map(|l| {
                let (t, id) = l.split_once('\t').unwrap();
                (t == tag).then(|| id.parse().unwrap())
            })
            .collect();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}
