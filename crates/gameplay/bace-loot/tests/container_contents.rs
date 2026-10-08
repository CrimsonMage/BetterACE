//! Independent unchanged ACE Container methods: GenerateContainList and TryAdd.
use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_loot::materialize_container_tree;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
fn template(value: &Value) -> WeenieV1 {
    let id = value["id"].as_u64().unwrap() as u32;
    let mut w = WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("container_fixture_{id}"),
        weenie_type: value["type"].as_u64().unwrap() as u32,
        last_modified: None,
        properties: Default::default(),
    };
    w.properties.ints = vec![
        Property { id: 3, value: 2 },
        Property {
            id: 6,
            value: value["main"].as_i64().unwrap() as i32,
        },
        Property {
            id: 7,
            value: value["pack"].as_i64().unwrap() as i32,
        },
        Property { id: 11, value: 100 },
        Property { id: 12, value: 1 },
    ];
    w.properties.floats = vec![Property {
        id: 12,
        value: 0.75,
    }];
    w.properties.bools = vec![Property {
        id: 81,
        value: value["side"].as_bool().unwrap(),
    }];
    w.properties.create_list = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(n, r)| CreateListEntry {
            database_record_id: n as u32 + 1,
            destination_type: r["dest"].as_i64().unwrap() as i32,
            weenie_class_id: r["id"].as_u64().unwrap() as u32,
            stack_size: r["stack"].as_i64().unwrap() as i32,
            palette: r["palette"].as_i64().unwrap() as i8,
            shade: r["shade"].as_f64().unwrap() as f32,
            try_to_bond: false,
        })
        .collect();
    w
}
#[test]
fn original_container_methods_preserve_shade_capacity_parent_and_unstable_fallback_order() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/container_contents.json")).unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 45);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let templates: BTreeMap<_, _> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(template)
            .map(|w| (w.weenie_id, Arc::new(w)))
            .collect();
        let actual =
            materialize_container_tree(templates[&1].as_ref().clone(), &templates).unwrap();
        assert_eq!(actual[0].source_destination, None);
        for item in actual.iter().skip(1) {
            let source_parent = item.generator_parent_index.expect("generated child source");
            assert!(
                templates[&actual[source_parent].source.weenie_id]
                    .properties
                    .create_list
                    .iter()
                    .any(|row| {
                        row.weenie_class_id == item.source.weenie_id
                            && u8::try_from(row.destination_type).ok() == item.source_destination
                    })
            );
        }
        let mut observed:Vec<_>=actual.iter().map(|item|{let w=&item.source;let int=|id|w.properties.ints.iter().find(|p|p.id==id).map_or(0,|p|p.value);json!({"id":w.weenie_id,"parent":item.parent_index.map(|p|actual[p].source.weenie_id),"generator":item.generator_parent_index.map(|p|actual[p].source.weenie_id),"slot":item.inventory_slot,"palette":int(3),"shade":w.properties.floats.iter().find(|p|p.id==12).unwrap().value,"stack":int(12)})}).collect();
        let mut expected = case["output"].as_array().unwrap().clone();
        for value in &mut expected {
            value["shade"] = json!(value["shade"].as_f64().unwrap());
        }
        observed.sort_by_key(|v| v["id"].as_u64());
        expected.sort_by_key(|v| v["id"].as_u64());
        assert_eq!(observed, expected, "fixture {index}");
    }
}
#[test]
fn mutated_root_is_retained_and_recursive_cycles_fail_without_partial_output() {
    let mut root = template(&json!({"id":1,"type":21,"main":5,"pack":0,"side":false,"rows":[]}));
    root.properties.strings.push(Property {
        id: 16,
        value: "already rolled rare properties".into(),
    });
    root.properties.floats[0].value = 0.3125;
    let tree = materialize_container_tree(root.clone(), &BTreeMap::new()).unwrap();
    assert_eq!(tree[0].source, root);
    root.properties.create_list.push(CreateListEntry {
        database_record_id: 1,
        destination_type: 1,
        weenie_class_id: 1,
        stack_size: 1,
        palette: 0,
        shade: 0.,
        try_to_bond: false,
    });
    let templates = BTreeMap::from([(1, Arc::new(root.clone()))]);
    assert!(materialize_container_tree(root, &templates).is_err());
}
