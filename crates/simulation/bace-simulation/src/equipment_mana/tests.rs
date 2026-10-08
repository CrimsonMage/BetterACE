use super::*;
#[test]
fn mana_float_bits_warnings_and_depletion_match_original_ace_heartbeat() {
    let mut rows = 0;
    for line in include_str!("../../tests/fixtures/equipment_mana.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let b = |index: usize| f[index] == "True";
        let item = EquipmentManaItem {
            item: EntityId(2),
            name: "Test".into(),
            current: Some(f[0].parse().unwrap()),
            maximum: Some(100),
            rate: Some(f[1].parse().unwrap()),
            affecting: Some(true),
            removals: vec![],
            accumulator: f32::from_bits(f[5].parse().unwrap()),
            warned: b(4),
            removal_remaining: None,
        };
        let (after, notice) =
            heartbeat(&item, f[2].parse().unwrap(), f[3].parse().unwrap()).unwrap();
        assert_eq!(after.current, Some(f[6].parse().unwrap()), "{line}");
        assert_eq!(
            after.accumulator.to_bits(),
            f[7].parse::<u32>().unwrap(),
            "{line}"
        );
        assert_eq!(after.warned, b(8), "{line}");
        assert_eq!(after.affecting == Some(true), b(9), "{line}");
        assert_eq!(
            usize::from(notice == Some(false)),
            f[10].parse::<usize>().unwrap(),
            "{line}"
        );
        assert_eq!(
            usize::from(notice == Some(true)),
            f[11].parse::<usize>().unwrap(),
            "{line}"
        );
        assert_eq!(
            usize::from(notice == Some(true)),
            f[12].parse::<usize>().unwrap(),
            "{line}"
        );
        assert_eq!(
            after.removal_remaining.unwrap_or(0.),
            f[13].parse::<f64>().unwrap(),
            "{line}"
        );
        rows += 1;
    }
    assert_eq!(rows, 1080);
}
