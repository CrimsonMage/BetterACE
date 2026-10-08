use bace_magic::*;
fn registry(family: u32, key: u32, sign: f32) -> EnchantmentRegistry {
    let mut registry = EnchantmentRegistry::new(8).unwrap();
    for (i, (kind, value)) in [
        (0x4000, 1.2),
        (0x4000, 0.7),
        (0x8000, sign * 13.),
        (0x8000, -4.),
    ]
    .into_iter()
    .enumerate()
    {
        registry
            .add(
                EnchantmentEntry {
                    spell: i as u32 + 1,
                    caster: 1,
                    school: MagicSchool::Item,
                    spec: EnchantmentSpec {
                        category: i as u16 + 1,
                        power: 100,
                        duration: 30.,
                        layer: 0,
                        stat_type: family | 0x1000 | kind,
                        stat_key: key,
                        value,
                        beneficial: true,
                        set_id: None,
                    },
                    start_time: 0.,
                    is_set_spell: false,
                    is_level8_aura: false,
                    metadata: EnchantmentMetadata::default(),
                },
                0.,
                false,
            )
            .unwrap();
    }
    registry
}
#[test]
fn unchanged_gdle_scalar_and_details_methods_match_exact_bits() {
    for line in include_str!("fixtures/physical_quality.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let parts: Vec<_> = line.split(',').collect();
        let raw: f64 = parts[0].parse().unwrap();
        let sign: f32 = parts[1].parse().unwrap();
        let (details, integer) =
            enchant_physical_quality(&registry(4, 44, sign), 4, 44, raw, true).unwrap();
        assert_eq!(integer, parts[2].parse::<f64>().unwrap());
        let (_, float) =
            enchant_physical_quality(&registry(8, 62, sign), 8, 62, raw, false).unwrap();
        assert_eq!(float.to_bits(), parts[3].parse::<u64>().unwrap());
        for (actual, expected) in [
            details.increasing,
            details.decreasing,
            details.additive_increasing,
            details.additive_decreasing,
        ]
        .into_iter()
        .zip(&parts[4..])
        {
            assert_eq!(actual.to_bits(), expected.parse::<u64>().unwrap());
        }
    }
    assert!(enchant_physical_quality(&registry(4, 44, 1.), 4, 44, f64::NAN, true).is_err());
}
