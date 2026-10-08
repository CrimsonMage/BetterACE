use bace_interactions::{
    PlayerDeathKind, PlayerDeathPolicy, PortalPosition, classify_player_death, death_item_count,
    recall_moved_too_far,
};
#[test]
fn unchanged_compiled_ace_death_and_recall_methods() {
    let mut counts = [0; 4];
    for line in include_str!("fixtures/ace_world_policy.tsv").lines() {
        let p: Vec<_> = line.split('\t').collect();
        let uint = |i: usize| p[i].parse::<u32>().unwrap();
        match p[0] {
            "V" => {
                let max = p[2].parse::<f64>().unwrap();
                let policy = PlayerDeathPolicy {
                    vitae_penalty: 0.,
                    vitae_penalty_max: max,
                    ..Default::default()
                };
                let actual = policy.next_vitae(uint(1), Some(0.)).unwrap();
                assert_eq!(
                    actual.to_bits(),
                    p[3].parse::<f32>().unwrap().to_bits(),
                    "{line}"
                );
                counts[0] += 1;
            }
            "I" => {
                assert_eq!(
                    death_item_count(uint(1), uint(4), uint(2), uint(3) != 0).unwrap(),
                    uint(5),
                    "{line}"
                );
                counts[1] += 1;
            }
            "K" => {
                let actual = match classify_player_death(0x50000001, uint(1), Some(uint(2))) {
                    PlayerDeathKind::Ordinary => 0,
                    PlayerDeathKind::Pk => 1,
                    PlayerDeathKind::Pkl => 2,
                };
                assert_eq!(actual, uint(3), "{line}");
                counts[2] += 1;
            }
            "M" => {
                let start = PortalPosition {
                    cell: 1,
                    origin: [0.; 3],
                    rotation: [1., 0., 0., 0.],
                };
                let end = PortalPosition {
                    cell: (uint(1) << 16) | 1,
                    origin: [p[2].parse().unwrap(), 0., 0.],
                    ..start
                };
                assert_eq!(
                    recall_moved_too_far(start, end).unwrap(),
                    uint(3) != 0,
                    "{line}"
                );
                counts[3] += 1;
            }
            "L" | "D" | "X" => {}
            _ => panic!("invalid fixture"),
        }
    }
    assert_eq!(counts, [1100, 6440, 768, 20]);
}
