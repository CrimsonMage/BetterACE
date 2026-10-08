use super::*;
use bace_simulation::{
    PreparedVendorLazyItem, PreparedVendorLazyStock, PreparedVendorTree, VendorLazyStockReceipt,
};
use std::collections::BTreeMap;
fn tree(root: EntityId, child: EntityId, stack: i32) -> PreparedVendorTree {
    let mut bag = (*stock(stack)).clone();
    bag.weenie_type = 21;
    bag.properties.ints.push(Property { id: 6, value: 10 });
    PreparedVendorTree {
        root,
        template: Arc::new(bag),
        items: vec![item(child.0, root.0, false)],
        containers: vec![container(root.0, None)],
        templates: BTreeMap::from([(child, stock(1))]),
    }
}
fn vendor() -> (Kernel, GeneratorIdentity) {
    let mut k = kernel();
    let mut d = definition(20, 32);
    d.initial_count = 2;
    d.maximum_count = 2;
    d.profiles[0].max_create = -1;
    k.register_generator_vendor(EntityId(20), 10).unwrap();
    k.register_generator(Arc::new(d.clone())).unwrap();
    (k, d.identity)
}
#[test]
fn vendor_retains_explicit_child_tree_until_last_contribution_is_destroyed() {
    let (mut k, identity) = vendor();
    let req = request(&mut k, 0x80001001);
    let root = req.entities[0];
    let child = EntityId(0x80001002);
    k.supply_generator_request_ids(req.intent.key, 1, &[child])
        .unwrap();
    let accepted = k
        .admit_generated_vendor_trees(req.intent.key, &[tree(root, child, 5)])
        .unwrap();
    assert_eq!(accepted.entities, [root, child]);
    assert_eq!(accepted.roots, [root]);
    let contents = k.generated_vendor_contents(EntityId(20), root).unwrap();
    assert!(
        matches!(contents.items[0].place, ItemPlace::Contained { container, slot: 0, equipped: 0 } if container == root)
    );
    assert!(k.generated_vendor_item(EntityId(20), child.0).is_some());
    assert!(k.supply_generator_id(child).is_err());
    assert!(
        k.inventory_item(child).is_none(),
        "stock has one vendor owner"
    );
    k.generator_control(identity, GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    assert!(k.generated_vendor_contents(EntityId(20), root).is_none());
    assert!(k.generated_vendor_item(EntityId(20), child.0).is_none());
    assert!(!k.generator_reserves_identity(child));
}
#[test]
fn stock_merge_discards_whole_incoming_tree_and_retains_original_contents() {
    let (mut k, _) = vendor();
    let first = request(&mut k, 0x80001101);
    let root = first.entities[0];
    let child = EntityId(0x80001102);
    k.supply_generator_request_ids(first.intent.key, 1, &[child])
        .unwrap();
    k.admit_generated_vendor_trees(first.intent.key, &[tree(root, child, 5)])
        .unwrap();
    let second = request(&mut k, 0x80001103);
    let incoming = second.entities[0];
    let discarded = EntityId(0x80001104);
    k.supply_generator_request_ids(second.intent.key, 1, &[discarded])
        .unwrap();
    let accepted = k
        .admit_generated_vendor_trees(second.intent.key, &[tree(incoming, discarded, 99)])
        .unwrap();
    assert_eq!(accepted.roots, [root]);
    assert!(accepted.entities.is_empty());
    assert!(accepted.failed_roots.is_empty());
    let golden = include_str!("../fixtures/vendor_tree.csv")
        .lines()
        .find(|line| line.starts_with("100|"))
        .unwrap();
    let fields: Vec<_> = golden.split('|').collect();
    assert_eq!(fields[1], "1");
    assert_eq!(
        fields[3], "101",
        "ACE retains only the original root contents"
    );
    assert_eq!(
        k.generated_vendor_stock(EntityId(20)).unwrap().items()[0].stack,
        Some(fields[2].parse().unwrap())
    );
    assert_eq!(
        k.generated_vendor_contents(EntityId(20), root)
            .unwrap()
            .items[0]
            .id,
        child
    );
    for id in [incoming, discarded] {
        assert!(k.generated_vendor_item(EntityId(20), id.0).is_none());
        assert!(!k.generator_reserves_identity(id));
    }
}
#[test]
fn malformed_stock_parent_cannot_partially_register_or_consume_request() {
    let (mut k, _) = vendor();
    let req = request(&mut k, 0x80001201);
    let root = req.entities[0];
    let child = EntityId(0x80001202);
    k.supply_generator_request_ids(req.intent.key, 1, &[child])
        .unwrap();
    let mut malformed = tree(root, child, 1);
    malformed.items[0].place = ItemPlace::Contained {
        container: child,
        slot: 0,
        equipped: 0,
    };
    assert_eq!(
        k.admit_generated_vendor_trees(req.intent.key, &[malformed]),
        Err(GeneratorServiceError::Invalid)
    );
    assert!(
        k.generated_vendor_stock(EntityId(20))
            .unwrap()
            .items()
            .is_empty()
    );
    assert!(k.generated_vendor_item(EntityId(20), child.0).is_none());
    k.admit_generated_vendor_trees(req.intent.key, &[tree(root, child, 1)])
        .unwrap();
}

#[test]
fn one_stock_receipt_preserves_input_order_and_duplicate_retained_roots() {
    let (mut k, identity) = vendor();
    let req = request(&mut k, 0x80001404);
    let ids = [
        EntityId(0x80001405),
        EntityId(0x80001402),
        EntityId(0x80001403),
    ];
    k.supply_generator_request_ids(req.intent.key, 1, &ids)
        .unwrap();
    let first = req.entities[0];
    let second = ids[1];
    let receipt = k
        .admit_generated_vendor_trees(
            req.intent.key,
            &[tree(first, ids[0], 5), tree(second, ids[2], 99)],
        )
        .unwrap();
    assert_eq!(
        receipt.roots,
        [first, first],
        "each incoming tree has an ordered retained identity"
    );
    assert_eq!(
        receipt.entities,
        [first, ids[0]],
        "the merged incoming tree is discarded"
    );
    assert_eq!(
        k.generated_vendor_stock(EntityId(20)).unwrap().items()[0].stack,
        Some(6)
    );
    assert!(!k.generator_reserves_identity(second));
    assert!(!k.generator_reserves_identity(ids[2]));
    k.generator_control(identity, GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    assert!(
        k.generated_vendor_stock(EntityId(20))
            .unwrap()
            .items()
            .is_empty(),
        "membership retains the sum of source contributions"
    );
    assert!(k.generated_vendor_contents(EntityId(20), first).is_none());
}

#[test]
fn lazy_shop_stock_remains_private_until_exact_durable_receipt() {
    let mut k = kernel();
    let vendor = EntityId(30);
    let marker = EntityId(0x8000_2000);
    let first = EntityId(0x8000_2001);
    let child = EntityId(0x8000_2002);
    let second = EntityId(0x8000_2003);
    k.register_vendor_stock(vendor, 10).unwrap();
    let batch = PreparedVendorLazyStock {
        vendor,
        vendor_expected_version: 3,
        marker,
        operation_id: "shop-load-30-1".into(),
        source_revision: 9,
        source_hash: [7; 32],
        expected_stock_revision: 0,
        entries: vec![
            PreparedVendorLazyItem {
                tree: tree(first, child, 5),
                display_quantity: -1,
                source_destinations: BTreeMap::from([(first, Some(4)), (child, Some(1))]),
            },
            PreparedVendorLazyItem {
                tree: PreparedVendorTree {
                    root: second,
                    template: stock(5),
                    items: vec![],
                    containers: vec![],
                    templates: BTreeMap::new(),
                },
                display_quantity: 7,
                source_destinations: BTreeMap::from([(second, Some(4))]),
            },
        ],
    };
    let ticket = k.reserve_vendor_lazy_stock(batch).unwrap();
    assert_eq!(ticket.item_ids, [first, child, second]);
    assert_eq!(ticket.vendor_expected_version, 3);
    assert!(k.vendor_lazy_pending(vendor));
    assert!(!k.vendor_stock_nonempty(vendor));
    assert!(k.generated_vendor_item(vendor, first.0).is_none());
    assert!(k.supply_generator_id(first).is_err());
    let mut receipt = VendorLazyStockReceipt {
        vendor,
        marker,
        operation_id: ticket.operation_id.clone(),
        marker_version: 1,
        items: ticket.item_ids.iter().map(|id| (*id, 1)).collect(),
    };
    receipt.items[1].1 = 2;
    assert!(k.confirm_vendor_lazy_stock(receipt.clone()).is_err());
    assert!(k.vendor_lazy_pending(vendor));
    assert!(!k.vendor_stock_nonempty(vendor));
    receipt.items[1].1 = 1;
    k.confirm_vendor_lazy_stock(receipt.clone()).unwrap();
    k.confirm_vendor_lazy_stock(receipt).unwrap();
    assert!(!k.vendor_lazy_pending(vendor));
    assert_eq!(k.generated_vendor_stock(vendor).unwrap().items().len(), 2);
    assert_eq!(k.vendor_stock_display_quantity(vendor, first), Some(-1));
    assert_eq!(k.vendor_stock_display_quantity(vendor, second), Some(7));
    assert_eq!(
        k.generated_vendor_contents(vendor, first).unwrap().items[0].id,
        child
    );
}

#[test]
fn lazy_shop_requires_durable_vendor_and_valid_source_flags() {
    let mut k = kernel();
    let vendor = EntityId(30);
    let root = EntityId(0x8000_2011);
    k.register_vendor_stock(vendor, 1).unwrap();
    let mut batch = PreparedVendorLazyStock {
        vendor,
        vendor_expected_version: 0,
        marker: EntityId(0x8000_2010),
        operation_id: "shop-load-30-2".into(),
        source_revision: 1,
        source_hash: [1; 32],
        expected_stock_revision: 0,
        entries: vec![PreparedVendorLazyItem {
            tree: PreparedVendorTree {
                root,
                template: stock(1),
                items: vec![],
                containers: vec![],
                templates: BTreeMap::new(),
            },
            display_quantity: 1,
            source_destinations: BTreeMap::from([(root, Some(4))]),
        }],
    };
    assert!(k.reserve_vendor_lazy_stock(batch.clone()).is_err());
    batch.vendor_expected_version = 1;
    batch.entries[0].source_destinations.insert(root, Some(1));
    assert!(k.reserve_vendor_lazy_stock(batch.clone()).is_err());
    assert!(!k.vendor_lazy_pending(vendor));
    batch.entries[0].source_destinations.insert(root, Some(4));
    k.reserve_vendor_lazy_stock(batch).unwrap();
}

#[test]
fn empty_authored_shop_load_advances_marker_revision_without_item_id() {
    let mut k = kernel();
    let vendor = EntityId(30);
    k.register_vendor_stock(vendor, 1).unwrap();
    let ticket = k
        .reserve_vendor_lazy_stock(PreparedVendorLazyStock {
            vendor,
            vendor_expected_version: 1,
            marker: EntityId(0x8000_2020),
            operation_id: "empty-shop-load".into(),
            source_revision: 1,
            source_hash: [2; 32],
            expected_stock_revision: 0,
            entries: vec![],
        })
        .unwrap();
    assert_eq!(ticket.stock_revision, 1);
    assert!(ticket.item_ids.is_empty());
    assert!(!k.vendor_stock_durable(vendor));
    k.confirm_vendor_lazy_stock(VendorLazyStockReceipt {
        vendor,
        marker: ticket.marker,
        operation_id: ticket.operation_id,
        marker_version: 1,
        items: vec![],
    })
    .unwrap();
    assert!(k.vendor_stock_durable(vendor));
    assert_eq!(k.generated_vendor_stock(vendor).unwrap().revision(), 1);
}
