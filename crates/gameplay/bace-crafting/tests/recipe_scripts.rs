use bace_crafting::{
    CraftError, PropertyKey, PropertyKind as K, PropertyValue as V, apply_recipe_script,
    recipe_script_supported,
};
use std::collections::{BTreeMap, BTreeSet};
fn parse(text: &str) -> BTreeMap<PropertyKey, V> {
    text.split(';')
        .filter(|s| !s.is_empty())
        .map(|row| {
            let pieces: Vec<_> = row.split(':').collect();
            let id = pieces[1].parse().unwrap();
            let raw = pieces[2];
            let value = match pieces[0] {
                "I" => V::Int(raw.parse().unwrap()),
                "L" => V::Int64(raw.parse().unwrap()),
                "F" => V::Float(f64::from_bits(u64::from_str_radix(raw, 16).unwrap())),
                "B" => V::Bool(raw == "1"),
                "D" => V::DataId(raw.parse().unwrap()),
                _ => panic!("bad fixture"),
            };
            (
                PropertyKey {
                    kind: value.kind(),
                    id,
                },
                value,
            )
        })
        .collect()
}
#[test]
fn original_ace_parser_and_effect_engine_all_recipe_scripts() {
    let mut count = 0;
    let mut ids = BTreeSet::new();
    for row in include_str!("fixtures/recipe_scripts.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let id = u32::from_str_radix(fields[0], 16).unwrap();
        let mut actual = parse(fields[2]);
        let expected = parse(fields[3]);
        assert!(recipe_script_supported(id));
        apply_recipe_script(&mut actual, id).unwrap();
        assert_eq!(actual, expected, "script {} seed {}", fields[0], fields[1]);
        for (key, value) in &expected {
            if let V::Float(expected) = value {
                let V::Float(actual) = actual[key] else {
                    panic!()
                };
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "{} {} {key:?}",
                    fields[0],
                    fields[1]
                );
            }
        }
        count += 1;
        ids.insert(id);
    }
    assert_eq!(ids.len(), 44);
    assert_eq!(count, 393);
}
#[test]
fn unsupported_and_overflow_scripts_preserve_all_input() {
    let key = PropertyKey {
        kind: K::Int,
        id: 44,
    };
    let mut properties = BTreeMap::from([(key, V::Int(i32::MAX))]);
    let original = properties.clone();
    assert_eq!(
        apply_recipe_script(&mut properties, 0x3800001a),
        Err(CraftError::Overflow)
    );
    assert_eq!(properties, original);
    assert_eq!(
        apply_recipe_script(&mut properties, 0xdeadbeef),
        Err(CraftError::Unsupported)
    );
    assert_eq!(properties, original);
}
