use bace_combat::physical::{npc_missile_range, npc_reload_speed};
#[test]
fn unchanged_official_ace_npc_missile_methods() {
    let mut count = 0;
    for row in include_str!("fixtures/npc_missile.csv")
        .lines()
        .filter(|r| !r.starts_with('#'))
    {
        let fields: Vec<_> = row.split(',').collect();
        let actual = match fields[0] {
            "range" => npc_missile_range(fields[1].parse().unwrap()).unwrap(),
            "speed" => npc_reload_speed(fields[1].parse().unwrap(), fields[2].parse().unwrap()),
            other => panic!("unknown fixture row {other}"),
        };
        assert_eq!(
            actual.to_bits(),
            fields[3].parse::<f32>().unwrap().to_bits(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 28);
    for invalid in [f64::NAN, f64::INFINITY, -1.0, 1001.0] {
        assert!(npc_missile_range(invalid).is_err());
    }
}
