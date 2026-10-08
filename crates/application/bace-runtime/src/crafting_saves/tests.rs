use super::*;
#[test]
fn add_spell_preserves_existing_probability_and_uses_original_ace_default() {
    let key = PropertyKey {
        kind: PropertyKind::SpellBook,
        id: 7,
    };
    let mut count = 0;
    for line in include_str!("../../tests/fixtures/crafting_spellbook.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let columns: Vec<_> = line.split(',').collect();
        let mut props = SparseProperties::default();
        if columns[0] != "absent" {
            props.spell_book.push(Property {
                id: 7,
                value: f32::from_bits(columns[0].parse().unwrap()),
            });
        }
        // Another known spell's probability/order must survive the delta.
        props.spell_book.push(Property { id: 8, value: 0.75 });
        let before = crafting_properties(&props);
        let mut after = before.clone();
        after.insert(key, PropertyValue::SpellBook(true));
        apply_properties(&mut props, &before, &after, Some(0)).unwrap();
        assert_eq!(
            props
                .spell_book
                .iter()
                .find(|spell| spell.id == 7)
                .unwrap()
                .value
                .to_bits(),
            columns[1].parse::<u32>().unwrap()
        );
        assert_eq!(
            props.spell_book.len(),
            columns[3].parse::<usize>().unwrap() + 1
        );
        assert_eq!(
            props
                .spell_book
                .iter()
                .find(|spell| spell.id == 8)
                .unwrap()
                .value,
            0.75
        );
        assert_eq!(crafting_properties(&props), after);
        let saved = props.clone();
        apply_properties(&mut props, &after, &after, Some(0)).unwrap();
        assert_eq!(props, saved);
        count += 1;
    }
    assert_eq!(count, 5);
}
#[test]
fn invalid_spell_membership_never_creates_a_different_numeric_property() {
    let mut props = SparseProperties::default();
    let before = crafting_properties(&props);
    let mut after = before.clone();
    after.insert(
        PropertyKey {
            kind: PropertyKind::SpellBook,
            id: 7,
        },
        PropertyValue::Bool(true),
    );
    assert!(apply_properties(&mut props, &before, &after, Some(0)).is_err());
    assert!(props.spell_book.is_empty());
    assert!(props.bools.is_empty());
}

#[test]
fn native_script_count_must_match_structured_item_before_freezing() {
    let key = PropertyKey {
        kind: PropertyKind::Int,
        id: 171,
    };
    let mut props = SparseProperties::default();
    let before = crafting_properties(&props);
    let after = BTreeMap::from([(key, PropertyValue::Int(1))]);
    assert!(apply_properties(&mut props, &before, &after, Some(2)).is_err());
    assert!(props.ints.is_empty());
    apply_properties(&mut props, &before, &after, Some(1)).unwrap();
    assert_eq!(props.ints, vec![Property { id: 171, value: 1 }]);
    assert_eq!(crafting_properties(&props), after);
}
