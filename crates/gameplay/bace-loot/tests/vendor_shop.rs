use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_loot::materialize_vendor_shop_create_list;
use std::{collections::BTreeMap, sync::Arc};

fn item(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("vendor_item_{id}"),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}
fn row(id: u32, destination_type: i32, quantity: i32) -> CreateListEntry {
    CreateListEntry {
        database_record_id: id,
        destination_type,
        weenie_class_id: id,
        stack_size: quantity,
        palette: 0,
        shade: 0.0,
        try_to_bond: false,
    }
}

#[test]
fn pinned_lazy_shop_filters_exact_destination_and_does_not_merge_duplicates() {
    // Source: official ACE 47edade3 Vendor.LoadInventory/LoadInventoryItem:
    // exact DestinationType.Shop (4), factory-null skip, source-order insertion,
    // positive palette/shade override, and a separate display quantity.
    let mut vendor = item(50, 12);
    vendor.properties.create_list = vec![
        row(100, 1, 1),
        row(101, 4, -1),
        row(102, 12, 1),
        row(101, 4, 7),
        row(999, 4, 5),
    ];
    vendor.properties.create_list[3].palette = 5;
    vendor.properties.create_list[3].shade = 0.25;
    let mut stock = item(101, 1);
    stock.properties.ints = vec![
        Property { id: 11, value: 100 },
        Property { id: 12, value: 3 },
    ];
    let templates = BTreeMap::from([(101, Arc::new(stock))]);
    let output = materialize_vendor_shop_create_list(&vendor, &templates).unwrap();
    assert_eq!(output.len(), 2);
    assert_eq!(
        output
            .iter()
            .map(|tree| tree.display_quantity)
            .collect::<Vec<_>>(),
        [-1, 7]
    );
    assert_eq!(output[0].items[0].source_destination, Some(4));
    assert_eq!(output[1].items[0].source_destination, Some(4));
    assert!(
        output[0].items[0]
            .source
            .properties
            .ints
            .iter()
            .all(|p| p.id != 3)
    );
    assert!(
        output[1].items[0]
            .source
            .properties
            .ints
            .iter()
            .any(|p| p.id == 3 && p.value == 5)
    );
    assert!(
        output[1].items[0]
            .source
            .properties
            .floats
            .iter()
            .any(|p| p.id == 12 && p.value == 0.25)
    );
    assert!(output.iter().all(|tree| {
        tree.items[0]
            .source
            .properties
            .ints
            .iter()
            .any(|p| p.id == 12 && p.value == 3)
    }));
}

#[test]
fn lazy_shop_keeps_contain_descendants_and_bounds_bad_quantity() {
    let mut vendor = item(50, 12);
    vendor.properties.create_list = vec![row(200, 4, 1)];
    let mut bag = item(200, 21);
    bag.properties.ints.push(Property { id: 6, value: 4 });
    bag.properties.create_list.push(row(201, 1, 1));
    let templates = BTreeMap::from([(200, Arc::new(bag)), (201, Arc::new(item(201, 1)))]);
    let output = materialize_vendor_shop_create_list(&vendor, &templates).unwrap();
    assert_eq!(output[0].items.len(), 2);
    assert_eq!(output[0].items[1].parent_index, Some(0));
    assert_eq!(output[0].items[1].source_destination, Some(1));
    vendor.properties.create_list[0].stack_size = -2;
    assert!(materialize_vendor_shop_create_list(&vendor, &templates).is_err());
}
