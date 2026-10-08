use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
    enchantment_modifiers,
};
#[test]
fn original_compiled_ace_filter_then_category_ordering() {
    for row in include_str!("fixtures/quality_modifiers.tsv").lines() {
        let p: Vec<_> = row.split('\t').collect();
        let entries = p[2]
            .split(';')
            .enumerate()
            .map(|(i, entry)| {
                let e: Vec<_> = entry.split(',').collect();
                EnchantmentEntry {
                    spell: e[0].parse().unwrap(),
                    caster: 1,
                    school: MagicSchool::Creature,
                    spec: EnchantmentSpec {
                        category: e[1].parse().unwrap(),
                        power: e[2].parse().unwrap(),
                        duration: 120.,
                        layer: i as u16,
                        stat_type: e[3].parse().unwrap(),
                        stat_key: e[4].parse().unwrap(),
                        value: 1.,
                        beneficial: true,
                        set_id: None,
                    },
                    start_time: e[5].parse().unwrap(),
                    is_set_spell: e[6] == "1",
                    is_level8_aura: e[7] == "1",
                    metadata: EnchantmentMetadata::default(),
                }
            })
            .collect();
        let registry = EnchantmentRegistry::restore(64, 0, entries).unwrap();
        let selected: Vec<_> =
            enchantment_modifiers(&registry, p[0].parse().unwrap(), p[1].parse().unwrap())
                .iter()
                .map(|e| e.spell)
                .collect();
        let expected: Vec<u32> = if p[3].is_empty() {
            vec![]
        } else {
            p[3].split(',').map(|v| v.parse().unwrap()).collect()
        };
        assert_eq!(selected, expected, "{row}");
    }
    assert_eq!(
        include_str!("fixtures/quality_modifiers.tsv")
            .lines()
            .count(),
        512
    );
}
