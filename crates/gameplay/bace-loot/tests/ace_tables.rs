//! Every extracted row is compared to reflection of original C# initializers.
use bace_loot::ace_tables as t;
use std::collections::BTreeMap;
mod table_support;
#[test]
fn source_tables_match_compiled_csharp_values_order_and_float_bits() {
    table_support::install_tables();
    let mut counts = BTreeMap::new();
    let fixture = include_str!("fixtures/ace-tables.csv");
    for line in fixture
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split('|').collect();
        let key = (fields[0], fields[1]);
        let index = counts.entry(key).or_insert(0usize);
        let (class, field) = fields[1].split_once('.').unwrap();
        let int = |n: usize| fields[n].parse::<i64>().unwrap();
        let bits = |n: usize| fields[n].parse::<u32>().unwrap();
        match fields[0] {
            "chance" => {
                if let Some(rows) = t::lookup_float(class, field) {
                    assert_eq!(rows[*index].0.to_bits(), bits(2), "{line}");
                    assert_eq!(rows[*index].1.to_bits(), bits(3), "{line}");
                } else {
                    let rows = t::lookup(class, field).expect(line);
                    assert_eq!(rows[*index], (int(2), f32::from_bits(bits(3))), "{line}");
                }
            }
            "gem" => {
                let rows = t::gem(class, field).expect(line);
                assert_eq!(
                    rows[*index],
                    (int(2), int(3), f32::from_bits(bits(4))),
                    "{line}"
                );
            }
            "sequence" => assert_eq!(
                t::sequence(class, field).expect(line)[*index],
                int(2),
                "{line}"
            ),
            "reference" => assert_eq!(
                t::references(class, field).expect(line)[*index],
                fields[2],
                "{line}"
            ),
            "typed" => assert_eq!(
                (
                    t::typed_references(class, field).expect(line)[*index]
                        .0
                        .as_str(),
                    t::typed_references(class, field).expect(line)[*index].1
                ),
                (fields[2], int(3)),
                "{line}"
            ),
            "float" => assert_eq!(
                t::float_sequence(class, field).expect(line)[*index].to_bits(),
                bits(2),
                "{line}"
            ),
            "descriptor" => assert_eq!(
                (
                    t::descriptors(class, field).expect(line)[*index].0,
                    t::descriptors(class, field).expect(line)[*index].1.as_str()
                ),
                (int(2), fields[3]),
                "{line}"
            ),
            kind => panic!("unknown oracle kind {kind}"),
        }
        *index += 1;
    }
    assert!(counts.len() > 1200);
    for ((kind, key), count) in counts {
        let (class, field) = key.split_once('.').unwrap();
        let actual = match kind {
            "chance" => t::lookup(class, field)
                .map(<[_]>::len)
                .or_else(|| t::lookup_float(class, field).map(<[_]>::len)),
            "gem" => t::gem(class, field).map(<[_]>::len),
            "sequence" => t::sequence(class, field).map(<[_]>::len),
            "reference" => t::references(class, field).map(<[_]>::len),
            "typed" => t::typed_references(class, field).map(<[_]>::len),
            "float" => t::float_sequence(class, field).map(<[_]>::len),
            "descriptor" => t::descriptors(class, field).map(<[_]>::len),
            _ => unreachable!(),
        };
        assert_eq!(actual, Some(count), "{key}");
    }
}

#[test]
fn pinned_update_position_flags_exclude_csharp_attributes() {
    // Pinned ACE Source/ACE.Entity/Enum/UpdatePositionFlag.cs. The source has
    // SuppressMessage attributes containing commas before five members.
    table_support::install_tables();
    let expected = [
        ("None", 0),
        ("Velocity", 1),
        ("Placement", 2),
        ("Contact", 4),
        ("ZeroQw", 8),
        ("ZeroQx", 16),
        ("ZeroQy", 32),
        ("ZeroQz", 64),
    ];
    assert_eq!(
        t::enum_members("UpdatePositionFlag").unwrap(),
        expected.to_vec()
    );
}
