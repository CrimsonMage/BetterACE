use bace_content::{Property, SparseProperties, WeenieV1};
fn template(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: "oracle".into(),
        weenie_type: kind,
        last_modified: None,
        properties: SparseProperties::default(),
    }
}
#[test]
fn original_ace_fallback_selection_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/tinker_selection.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let row: Vec<u32> = line.split('\t').map(|v| v.parse().unwrap()).collect();
        let n = row[1];
        let mut source = template(row[0], 1);
        source.properties.ints.push(Property {
            id: 131,
            value: if n == 23 { 64 } else { 0 },
        });
        let mut target = template(1, [1, 2, 3, 6, 35, 5][n as usize % 6]);
        target.properties.ints = vec![
            Property {
                id: 19,
                value: if n.is_multiple_of(3) { 0 } else { 10 },
            },
            Property {
                id: 5,
                value: if n % 3 == 1 { 0 } else { 10 },
            },
            Property {
                id: 108,
                value: if n % 3 == 2 { 0 } else { 10 },
            },
            Property {
                id: 9,
                value: match n % 4 {
                    0 => 0x04000000,
                    1 => 1,
                    2 => 32,
                    _ => 257,
                },
            },
            Property {
                id: 1,
                value: if n.is_multiple_of(3) { 2 } else { 4 },
            },
            Property {
                id: 28,
                value: if n.is_multiple_of(5) { 0 } else { 1 },
            },
            Property {
                id: 36,
                value: if n == 23 { 9999 } else { 0 },
            },
        ];
        if n < 12 {
            target.properties.ints.push(Property { id: 105, value: 5 });
        }
        assert_eq!(
            bace_crafting::select_new_tinkering_recipe(&source, &target).unwrap_or(0),
            row[2],
            "{line}"
        );
        count += 1;
    }
    assert!(count > 2000);
}
