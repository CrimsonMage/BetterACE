use bace_magic::*;
fn entry(
    id: u32,
    category: u16,
    caster: u32,
    key: u32,
    value: f32,
    power: u32,
) -> EnchantmentEntry {
    EnchantmentEntry {
        spell: id,
        caster,
        school: MagicSchool::Void,
        spec: EnchantmentSpec {
            category,
            power,
            duration: 10.,
            layer: 1,
            stat_type: 0x9004,
            stat_key: key,
            value,
            beneficial: false,
            set_id: None,
        },
        start_time: 0.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    }
}
#[test]
fn original_periodic_calls_select_last_source_and_repeat_complete_hot_aggregate() {
    for row in include_str!("fixtures/periodic_native.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<u32> = row.split(',').map(|v| v.parse().unwrap()).collect();
        let mut entries = vec![
            entry(100, 685, 2, 318, 10., if p[0] != 0 { 300 } else { 100 }),
            entry(101, 685, 3, 318, 20., 200),
            entry(102, 631, 2, 318, 5., 100),
            entry(103, 636, 2, 330, 4., 100),
            entry(104, 637, 3, 330, 7., 100),
        ];
        for i in 0..p[2] {
            entries.push(entry(
                200 + i,
                if i % 2 == 0 { 617 } else { 630 },
                2,
                312,
                3. + (i * i) as f32,
                100 + i,
            ));
        }
        let registry = EnchantmentRegistry::restore(16, 0, entries).unwrap();
        let effects = gdle_periodic_batches(&registry, |id| {
            Some(if matches!(id, 100 | 101 | 103 | 104) {
                0x10000
            } else {
                0
            })
        })
        .unwrap();
        assert_eq!(
            (
                effects[0].amount,
                effects[0].source.filter(|_| p[1] != 0).unwrap_or(0)
            ),
            (p[3], p[4]),
            "{row}"
        );
        assert_eq!(
            (
                effects[1].amount,
                effects[1].source.filter(|_| p[1] != 0).unwrap_or(0)
            ),
            (p[5], p[6]),
            "{row}"
        );
        assert_eq!(effects.len() - 2, p[7] as usize);
        assert_eq!(
            effects[2..].iter().map(|e| e.amount).sum::<u32>(),
            p[8],
            "{row}"
        );
    }
}
