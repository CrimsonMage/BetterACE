use super::*;
use crate::{PreparedVendorLazyItem, PreparedVendorTree};
use bace_content::{Property, WeenieV1};

fn source() -> Arc<WeenieV1> {
    let mut value = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "vendor_stock_test".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    value.properties.ints = vec![
        Property { id: 11, value: 10 },
        Property { id: 12, value: 1 },
    ];
    Arc::new(value)
}

fn batch() -> PreparedVendorLazyStock {
    let root = EntityId(0x8000_2002);
    PreparedVendorLazyStock {
        vendor: EntityId(0x8000_2000),
        vendor_expected_version: 2,
        marker: EntityId(0x8000_2001),
        operation_id: "vendor-restore-test".into(),
        source_revision: 7,
        source_hash: [3; 32],
        expected_stock_revision: 0,
        entries: vec![PreparedVendorLazyItem {
            tree: PreparedVendorTree {
                root,
                template: source(),
                items: vec![],
                containers: vec![],
                templates: BTreeMap::new(),
            },
            display_quantity: -1,
            source_destinations: BTreeMap::from([(root, Some(4))]),
        }],
    }
}

#[test]
fn first_load_and_repeat_adoption_keep_one_stock_identity_and_durable_unload_handoff() {
    let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
    let batch = batch();
    kernel.register_vendor_stock(batch.vendor, 10).unwrap();
    let receipt = VendorLazyStockReceipt {
        vendor: batch.vendor,
        marker: batch.marker,
        operation_id: batch.operation_id.clone(),
        marker_version: 1,
        items: vec![(batch.entries[0].tree.root, 1)],
    };
    let first = kernel
        .adopt_vendor_loaded_stock(batch.clone(), receipt.clone())
        .unwrap();
    assert_eq!(first.item_ids, [batch.entries[0].tree.root]);
    assert!(kernel.vendor_stock_durable(batch.vendor));
    let repeat = kernel
        .adopt_vendor_loaded_stock(batch.clone(), receipt.clone())
        .unwrap();
    assert_eq!(repeat, first);
    assert_eq!(
        kernel
            .generated_vendor_stock(batch.vendor)
            .unwrap()
            .items()
            .len(),
        1
    );
    let mut stale = batch;
    stale.source_hash = [4; 32];
    assert_eq!(
        kernel.adopt_vendor_loaded_stock(stale, receipt),
        Err(G::Stale)
    );
    assert!(kernel.vendor_stock_durable(first.vendor));
}

#[test]
fn later_default_buy_marker_reopens_base_stock_and_rejects_stale_repeat() {
    let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
    let batch = batch();
    kernel.register_vendor_stock(batch.vendor, 10).unwrap();
    let receipt = VendorLazyStockReceipt {
        vendor: batch.vendor,
        marker: batch.marker,
        operation_id: batch.operation_id.clone(),
        marker_version: 2,
        items: vec![(batch.entries[0].tree.root, 1)],
    };
    let ticket = kernel
        .adopt_vendor_loaded_stock(batch.clone(), receipt.clone())
        .unwrap();
    let owner = &kernel.generated_vendors[&batch.vendor];
    assert_eq!(ticket.stock_revision, 2);
    assert_eq!(owner.stock.revision(), 2);
    assert_eq!(owner.marker_version, 2);
    assert_eq!(owner.marker_stock_revision, 3);
    assert!(kernel.vendor_stock_durable(batch.vendor));
    assert_eq!(
        kernel.adopt_vendor_loaded_stock(batch.clone(), receipt.clone()),
        Ok(ticket)
    );
    let mut stale = receipt;
    stale.marker_version = 1;
    assert_eq!(
        kernel.adopt_vendor_loaded_stock(batch, stale),
        Err(G::Stale)
    );
}

#[test]
fn empty_later_marker_reopens_without_inventing_stock_and_fresh_confirm_stays_v1() {
    let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
    let mut batch = batch();
    batch.entries.clear();
    kernel.register_vendor_stock(batch.vendor, 10).unwrap();
    let later = VendorLazyStockReceipt {
        vendor: batch.vendor,
        marker: batch.marker,
        operation_id: batch.operation_id.clone(),
        marker_version: 2,
        items: vec![],
    };
    kernel
        .adopt_vendor_loaded_stock(batch.clone(), later.clone())
        .unwrap();
    let owner = &kernel.generated_vendors[&batch.vendor];
    assert!(owner.stock.items().is_empty());
    assert_eq!(owner.stock.revision(), 1);
    assert_eq!(owner.marker_stock_revision, 2);
    assert!(kernel.vendor_stock_durable(batch.vendor));

    let mut fresh = crate::synthetic_scenario(1, 0).unwrap();
    fresh.register_vendor_stock(batch.vendor, 10).unwrap();
    fresh.reserve_vendor_lazy_stock(batch).unwrap();
    assert_eq!(fresh.confirm_vendor_lazy_stock(later), Err(G::Stale));
    assert!(!fresh.vendor_stock_durable(owner.committed_lazy.as_ref().unwrap().vendor));
}
