use super::*;
use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_storage_codec::{ItemSaveV5, VendorStockSaveV1};

fn source(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("vendor_source_{id}"),
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

fn vendor() -> WeenieV1 {
    let mut vendor = source(50, 12);
    vendor.properties.ints = vec![
        Property { id: 74, value: 1 },
        Property { id: 75, value: 0 },
        Property {
            id: 76,
            value: 1000,
        },
    ];
    vendor.properties.floats = vec![
        Property { id: 37, value: 0.5 },
        Property { id: 38, value: 1.0 },
    ];
    vendor
}

fn input(
    vendor: WeenieV1,
    ids: Vec<EntityId>,
    templates: BTreeMap<u32, Arc<WeenieV1>>,
) -> preparation::Input {
    preparation::Input {
        vendor: EntityId(0x8000_1000),
        vendor_expected_version: 3,
        source: vendor,
        source_revision: 7,
        source_hash: [9; 32],
        world_epoch: 2,
        max_message_bytes: 64 * 1024,
        operation_id: "vendor-load-test".into(),
        ids,
        templates,
    }
}

#[test]
fn zero_row_shop_first_use_still_has_exact_marker_receipt() {
    let prepared = preparation::prepare(input(
        vendor(),
        vec![EntityId(0x8000_1001)],
        BTreeMap::new(),
    ))
    .unwrap();
    assert!(prepared.batch.entries.is_empty());
    let ticket = VendorLazyStockTicket {
        vendor: prepared.batch.vendor,
        vendor_expected_version: 3,
        marker: prepared.batch.marker,
        operation_id: prepared.batch.operation_id.clone(),
        source_revision: 7,
        source_hash: [9; 32],
        expected_stock_revision: 0,
        stock_revision: 1,
        item_ids: vec![],
    };
    let operation = prepared.freeze(&ticket, 2).unwrap();
    assert!(operation.inventory.snapshots.is_empty());
    assert_eq!(operation.inventory.participants, [0x8000_1000, 0x8000_1001]);
    let marker = VendorStockSaveV1::decode(&operation.marker.bytes).unwrap();
    assert!(marker.loaded);
    assert_eq!(marker.stock_revision, 1);
    assert!(marker.defaults.is_empty());
    assert_eq!(
        receipt(&ticket, OperationOutcome::AlreadyCommitted)
            .unwrap()
            .marker_version,
        1
    );
}

#[test]
fn authored_shop_tree_freezes_v5_origin_and_parent_before_receipt() {
    let mut vendor_source = vendor();
    vendor_source.properties.create_list.push(row(100, 4, -1));
    let mut bag = source(100, 21);
    bag.properties.ints.push(Property { id: 6, value: 2 });
    bag.properties.create_list.push(row(101, 1, 1));
    let templates = BTreeMap::from([(100, Arc::new(bag)), (101, Arc::new(source(101, 1)))]);
    let ids = [
        EntityId(0x8000_1001),
        EntityId(0x8000_1002),
        EntityId(0x8000_1003),
    ];
    let prepared = preparation::prepare(input(vendor_source, ids.to_vec(), templates)).unwrap();
    assert_eq!(
        prepared.batch.entries[0].source_destinations[&ids[1]],
        Some(4)
    );
    assert_eq!(
        prepared.batch.entries[0].source_destinations[&ids[2]],
        Some(1)
    );
    let ticket = VendorLazyStockTicket {
        vendor: prepared.batch.vendor,
        vendor_expected_version: 3,
        marker: ids[0],
        operation_id: prepared.batch.operation_id.clone(),
        source_revision: 7,
        source_hash: [9; 32],
        expected_stock_revision: 0,
        stock_revision: 2,
        item_ids: ids[1..].to_vec(),
    };
    let operation = prepared.freeze(&ticket, 2).unwrap();
    let marker = VendorStockSaveV1::decode(&operation.marker.bytes).unwrap();
    assert_eq!(marker.defaults[0].root, ids[1].0);
    assert_eq!(marker.defaults[0].child_ids, [ids[2].0]);
    let root = ItemSaveV5::decode(&operation.inventory.snapshots[0].bytes).unwrap();
    let child = ItemSaveV5::decode(&operation.inventory.snapshots[1].bytes).unwrap();
    assert_eq!(root.source_destination, Some(4));
    assert_eq!(child.source_destination, Some(1));
    assert!(
        matches!(child.placement, bace_storage_codec::ItemPlacementV2::Contained {container,..} if container == ids[1].0)
    );
    assert_eq!(operation.inventory.changes.len(), 2);
    let mut wrong = ticket.clone();
    wrong.item_ids.reverse();
    assert!(
        preparation::prepare(input(vendor(), vec![ids[0]], BTreeMap::new()))
            .unwrap()
            .freeze(&wrong, 2)
            .is_err()
    );
}

#[test]
fn cold_loaded_shop_restores_exact_order_and_marker_without_reroll() {
    use bace_persistence::{
        StoredAggregate, StoredVendorStock, StoredVendorStockForest, StoredVendorStockItem,
    };
    let mut vendor_source = vendor();
    vendor_source.properties.create_list.push(row(100, 4, 3));
    let templates = BTreeMap::from([(100, Arc::new(source(100, 1)))]);
    let ids = [EntityId(0x8000_1101), EntityId(0x8000_1102)];
    let prepared =
        preparation::prepare(input(vendor_source.clone(), ids.to_vec(), templates)).unwrap();
    let ticket = VendorLazyStockTicket {
        vendor: prepared.batch.vendor,
        vendor_expected_version: 3,
        marker: ids[0],
        operation_id: prepared.batch.operation_id.clone(),
        source_revision: 7,
        source_hash: [9; 32],
        expected_stock_revision: 0,
        stock_revision: 2,
        item_ids: vec![ids[1]],
    };
    let operation = prepared.freeze(&ticket, 2).unwrap();
    let forest = StoredVendorStockForest {
        marker: StoredVendorStock {
            vendor_object_id: ticket.vendor.0,
            marker_object_id: ticket.marker.0,
            persisted_version: 1,
            bytes: operation.marker.bytes,
        },
        items: operation
            .inventory
            .snapshots
            .into_iter()
            .zip(operation.inventory.changes)
            .map(|(snapshot, change)| StoredVendorStockItem {
                aggregate: StoredAggregate {
                    object_id: snapshot.object_id,
                    persisted_version: 1,
                    bytes: snapshot.bytes,
                },
                placement: change.destination,
            })
            .collect(),
    };
    let restore = |forest| {
        preparation::restore(preparation::RestoreInput {
            vendor: ticket.vendor,
            vendor_expected_version: 3,
            source: vendor_source.clone(),
            source_revision: 7,
            source_hash: [9; 32],
            max_message_bytes: 64 * 1024,
            forest,
        })
    };
    let restored = restore(forest.clone()).unwrap();
    assert_eq!(restored.batch.entries.len(), 1);
    assert_eq!(restored.batch.entries[0].tree.root, ids[1]);
    assert_eq!(restored.batch.entries[0].display_quantity, 3);
    assert_eq!(restored.receipt.items, [(ids[1], 1)]);
    assert_eq!(restored.listing.items[0].object_id, ids[1].0);

    let mut later = forest.clone();
    let mut marker = VendorStockSaveV1::decode(&later.marker.bytes).unwrap();
    marker.stock_revision += 1;
    later.marker.bytes = marker.encode().unwrap();
    later.marker.persisted_version = 2;
    let restored = restore(later.clone()).unwrap();
    assert_eq!(restored.receipt.marker_version, 2);
    assert_eq!(restored.receipt.items, [(ids[1], 1)]);
    assert_eq!(restored.batch.entries[0].tree.root, ids[1]);

    let mut wrong_revision = later.clone();
    marker.stock_revision -= 1;
    wrong_revision.marker.bytes = marker.encode().unwrap();
    assert!(restore(wrong_revision).is_err());
    marker.stock_revision += 1;
    marker.defaults[0].contribution_units = 1;
    let mut generated = later.clone();
    generated.marker.bytes = marker.encode().unwrap();
    assert!(restore(generated).is_err());
    marker.defaults[0].contribution_units = 0;
    marker.unique.push(bace_storage_codec::VendorUniqueStockV1 {
        root: 0x8000_1103,
        display_quantity: 1,
        sold_at_seconds: None,
        child_ids: vec![],
    });
    let mut unique = later.clone();
    unique.marker.bytes = marker.encode().unwrap();
    assert!(restore(unique).is_err());
    let mut mutated_item = later;
    mutated_item.items[0].aggregate.persisted_version = 2;
    assert!(restore(mutated_item).is_err());
}

#[test]
fn zero_row_shop_reopens_later_marker_without_stock() {
    use bace_persistence::{StoredVendorStock, StoredVendorStockForest};
    let prepared = preparation::prepare(input(
        vendor(),
        vec![EntityId(0x8000_1201)],
        BTreeMap::new(),
    ))
    .unwrap();
    let ticket = VendorLazyStockTicket {
        vendor: prepared.batch.vendor,
        vendor_expected_version: 3,
        marker: prepared.batch.marker,
        operation_id: prepared.batch.operation_id.clone(),
        source_revision: 7,
        source_hash: [9; 32],
        expected_stock_revision: 0,
        stock_revision: 1,
        item_ids: vec![],
    };
    let operation = prepared.freeze(&ticket, 2).unwrap();
    let mut marker = VendorStockSaveV1::decode(&operation.marker.bytes).unwrap();
    marker.stock_revision = 2;
    let restored = preparation::restore(preparation::RestoreInput {
        vendor: ticket.vendor,
        vendor_expected_version: 3,
        source: vendor(),
        source_revision: 7,
        source_hash: [9; 32],
        max_message_bytes: 64 * 1024,
        forest: StoredVendorStockForest {
            marker: StoredVendorStock {
                vendor_object_id: ticket.vendor.0,
                marker_object_id: ticket.marker.0,
                persisted_version: 2,
                bytes: marker.encode().unwrap(),
            },
            items: vec![],
        },
    })
    .unwrap();
    assert!(restored.batch.entries.is_empty());
    assert_eq!(restored.receipt.marker_version, 2);
    assert!(restored.receipt.items.is_empty());
}

#[tokio::test]
async fn confirmed_shop_use_waits_for_private_reliable_admission() {
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("vendor-publication").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("vendor test account");
    };
    let key = SessionKey {
        id: 77,
        generation: 1,
    };
    runtime.sessions.insert(
        key,
        Session {
            account,
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: None,
            failure: None,
        },
    );
    let vendor_id = EntityId(0x8000_2000);
    let listing = preparation::listing(&vendor(), vendor_id, &[]).unwrap();
    runtime.vendors.completion = Some(VendorCompletion {
        key,
        context: ActionContext {
            actor: EntityId(0x5000_0001),
            account: runtime.sessions[&key].account.id,
            session: bace_gameplay_api::SessionId(1),
            sequence: 9,
        },
        ticket: VendorLazyStockTicket {
            vendor: vendor_id,
            vendor_expected_version: 1,
            marker: EntityId(0x8000_2001),
            operation_id: "vendor-published".into(),
            source_revision: 1,
            source_hash: [1; 32],
            expected_stock_revision: 0,
            stock_revision: 1,
            item_ids: vec![],
        },
        listing,
    });
    runtime.project_vendor_output().unwrap();
    let Some(NetworkCommand::SendReliableBatch {
        key: sent,
        correlation,
        messages,
    }) = runtime.network_output.pop_front()
    else {
        panic!("vendor listing requires reliable private publication");
    };
    assert_eq!(sent, key);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, 9);
    assert!(runtime.vendors.completion.is_some());
    runtime
        .reliable_admissions
        .push_back((key, correlation, true));
    runtime.project_vendor_output().unwrap();
    assert!(runtime.vendors.completion.is_none());
    assert!(runtime.vendors.publication.is_none());
}
