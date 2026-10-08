use bace_content::{Property, WeenieV1};
use bace_inventory::ItemPlace;
use bace_runtime::stack_factory::prepare_split_stack;
use bace_types::EntityId;

#[test]
fn fresh_stack_matches_compiled_pinned_ace_defaults_and_set_stack_size() {
    for line in include_str!("../../../gameplay/bace-inventory/tests/fixtures/split_factory.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let values: Vec<Option<i32>> = line.split(',').map(|v| v.parse().ok()).collect();
        let mut template = WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "split_template".into(),
            weenie_type: 51,
            last_modified: None,
            properties: Default::default(),
        };
        for (index, id) in [12, 11, 19, 5, 13, 15].into_iter().enumerate() {
            if let Some(value) = values[index] {
                template.properties.ints.push(Property { id, value });
            }
        }
        template.properties.ints.sort_by_key(|p| p.id);
        let prepared = prepare_split_stack(
            &template,
            1,
            EntityId(0x80000001),
            3,
            ItemPlace::World,
            true,
        )
        .unwrap();
        assert_eq!(
            prepared.item.unit_burden,
            values[6].unwrap() as u32,
            "{line}"
        );
        assert_eq!(
            prepared.item.unit_value,
            values[7].unwrap() as u32,
            "{line}"
        );
        for (index, id) in [(8, 12), (9, 5), (10, 19)] {
            assert_eq!(
                prepared
                    .frozen
                    .entity
                    .state
                    .properties
                    .ints
                    .iter()
                    .find(|p| p.id == id)
                    .unwrap()
                    .value,
                values[index].unwrap(),
                "{line}"
            );
        }
        assert!(prepared.frozen.enchantments.is_empty());
        assert_eq!(prepared.frozen.persisted_version, 0);
        assert_eq!(prepared.item.place, ItemPlace::World);
    }
}
