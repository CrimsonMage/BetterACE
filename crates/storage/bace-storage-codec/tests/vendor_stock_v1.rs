use bace_storage_codec::{VendorDefaultStockV1, VendorStockSaveV1, VendorUniqueStockV1};

fn marker() -> VendorStockSaveV1 {
    VendorStockSaveV1 {
        marker_object_id: 0x8000_0100,
        vendor_object_id: 0x8000_0001,
        source_revision: 17,
        source_hash: [0x5a; 32],
        loaded: true,
        stock_revision: 3,
        defaults: vec![VendorDefaultStockV1 {
            root: 0x8000_0002,
            display_quantity: -1,
            contribution_units: 0,
            child_ids: vec![0x8000_0003, 0x8000_0004],
        }],
        unique: vec![VendorUniqueStockV1 {
            root: 0x8000_0005,
            display_quantity: 1,
            sold_at_seconds: Some(42),
            child_ids: vec![],
        }],
    }
}

#[test]
fn ordered_vendor_stock_is_a_bounded_unplaced_save() {
    let value = marker();
    let bytes = value.encode().unwrap();
    assert_eq!(VendorStockSaveV1::decode(&bytes).unwrap(), value);
    let mut changed = bytes.clone();
    *changed.last_mut().unwrap() ^= 1;
    assert!(VendorStockSaveV1::decode(&changed).is_err());

    let mut duplicate = value.clone();
    duplicate.unique[0].root = duplicate.defaults[0].child_ids[0];
    assert!(duplicate.encode().is_err());
    let mut invalid = value.clone();
    invalid.defaults[0].display_quantity = -2;
    assert!(invalid.encode().is_err());
    let mut invalid = value.clone();
    invalid.marker_object_id = invalid.vendor_object_id;
    assert!(invalid.encode().is_err());
    let mut invalid = value;
    invalid.loaded = false;
    assert!(invalid.encode().is_err());
}
