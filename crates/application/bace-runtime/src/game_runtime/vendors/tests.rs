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

#[test]
#[ignore = "requires approved DATs and accepted full native pack"]
fn approved_academy_shop_and_shoushi_location_are_source_backed() {
    use bace_content::WorldRecordV1;
    use bace_storage_codec::{PackKey, PackLookup};
    let dat = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("approved DAT directory"),
    )
    .join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&dat).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut archive = bace_dat::DatArchive::open(dat).unwrap();
    let chargen =
        bace_dat::CharGen::decode(&archive.read(bace_dat::CharGen::RECORD_ID).unwrap()).unwrap();
    let (index, shoushi) = chargen
        .starter_areas
        .iter()
        .enumerate()
        .find(|(_, area)| area.name == "Shoushi")
        .unwrap();
    let start = shoushi.locations.first().unwrap();
    let path = std::path::PathBuf::from(
        std::env::var_os("BACE_WORLD_MANIFEST").expect("accepted native pack"),
    );
    let manifest = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = manifest
        .open(path.parent().unwrap(), Default::default())
        .unwrap();
    let record = |namespace, id| match generation.lookup(PackKey { namespace, id }).unwrap() {
        PackLookup::Record(record) => record.bytes().to_vec(),
        _ => panic!("accepted source {namespace}:{id} missing"),
    };
    let WorldRecordV1::LandblockInstance(vendor) =
        bace_content_tools::decode_world_record(&record(20, 2012229732)).unwrap()
    else {
        panic!("academy provisioner instance");
    };
    let template: bace_content::WeenieV1 =
        bace_content_tools::decode(&record(1, vendor.weenie_class_id as u64)).unwrap();
    assert_eq!(template.weenie_type, 12);
    assert_eq!(vendor.obj_cell_id >> 16, start.cell >> 16);
    assert!(
        template
            .properties
            .create_list
            .iter()
            .any(|entry| entry.destination_type == 4)
    );
    eprintln!(
        "Shoushi DAT area={index} start={start:?}; authored vendor={} pose=({},{},{}) radius={:?}",
        vendor.guid,
        vendor.origin_x,
        vendor.origin_y,
        vendor.origin_z,
        template
            .properties
            .floats
            .iter()
            .find(|p| p.id == 54)
            .map(|p| p.value)
    );
}

/// The accepted template and DAT start stay immutable. Only one placement and
/// its copied landblock index are published into this test's private pack.
fn nearby_academy_shop_records() -> (Vec<bace_storage_codec::PackRecord>, u32) {
    use bace_content::{LandblockInstanceRowV1, WorldRecordV1};
    use bace_storage_codec::{PackKey, PackLookup, PackRecord};
    const VENDOR: u32 = 0x7f00_a001;
    const TEMPLATE: u32 = 12718;
    let dat = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let manifest = crate::region_activation::RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut portal = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let chargen =
        bace_dat::CharGen::decode(&portal.read(bace_dat::CharGen::RECORD_ID).unwrap()).unwrap();
    let (start_area, area) = chargen
        .starter_areas
        .iter()
        .enumerate()
        .find(|(_, a)| a.name == "Shoushi")
        .unwrap();
    let start = area.locations[0];
    let block = (start.cell >> 16) as u16;
    let path = std::path::PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").unwrap());
    let accepted = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = accepted
        .open(path.parent().unwrap(), Default::default())
        .unwrap();
    let lookup = |namespace, id| match generation.lookup(PackKey { namespace, id }).unwrap() {
        PackLookup::Record(record) => record.bytes().to_vec(),
        _ => panic!("accepted Shop source {namespace}:{id} missing"),
    };
    assert!(matches!(
        generation
            .lookup(PackKey {
                namespace: 20,
                id: VENDOR as u64
            })
            .unwrap(),
        PackLookup::Missing
    ));
    let template: bace_content::WeenieV1 =
        bace_content_tools::decode(&lookup(1, TEMPLATE as u64)).unwrap();
    assert_eq!(template.weenie_type, 12);
    let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest).unwrap();
    let (geometry, visibility) = assets.prepare_geometry_with_visibility(block).unwrap();
    let physical = assets.prepare_template(&template).unwrap();
    let visible = visibility
        .iter()
        .find(|row| row.cell.0 == start.cell)
        .map(|row| {
            row.visible_cells
                .iter()
                .map(|cell| cell.0)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let existing = crate::world_content::prepare(&generation, block).unwrap();
    let (cell, position) = [
        (2.0, 0.0),
        (-2.0, 0.0),
        (0.0, 2.0),
        (0.0, -2.0),
        (1.8, 1.8),
        (-1.8, 1.8),
        (1.8, -1.8),
        (-1.8, -1.8),
    ]
    .into_iter()
    .find_map(|(dx, dy)| {
        let position =
            bace_geometry::Vec3::new(start.origin[0] + dx, start.origin[1] + dy, start.origin[2]);
        if existing.instances.iter().any(|root| {
            let source = &root.source;
            (source.origin_z - position.z).abs() < 2.0
                && (source.origin_x - position.x).powi(2) + (source.origin_y - position.y).powi(2)
                    < 2.25
        }) {
            return None;
        }
        let cell = geometry
            .placement_cell(start.cell, position, &physical.shape, &visible)
            .ok()?;
        geometry
            .validate_placement(cell, position, &physical.shape, &[], VENDOR)
            .ok()?;
        Some((cell, position))
    })
    .expect("approved DAT has a clear Shop placement inside source use radius");
    let mut index = bace_content_tools::decode_landblock_index(&lookup(2, block as u64)).unwrap();
    assert!(!index.instance_ids.contains(&VENDOR));
    index.instance_ids.push(VENDOR);
    index.instance_ids.sort_unstable();
    let instance = WorldRecordV1::LandblockInstance(LandblockInstanceRowV1 {
        guid: VENDOR,
        landblock: i32::from(block),
        weenie_class_id: TEMPLATE,
        obj_cell_id: cell,
        origin_x: position.x,
        origin_y: position.y,
        origin_z: position.z,
        angles_w: 1.0,
        angles_x: 0.0,
        angles_y: 0.0,
        angles_z: 0.0,
        is_link_child: false,
        last_modified: "2021-11-01 00:00:00".into(),
    });
    (
        vec![
            PackRecord {
                key: PackKey {
                    namespace: 2,
                    id: block as u64,
                },
                schema: 1,
                value: Some(bace_content_tools::compile_landblock_index(&index).unwrap()),
            },
            PackRecord {
                key: PackKey {
                    namespace: 20,
                    id: VENDOR as u64,
                },
                schema: 1,
                value: Some(bace_content_tools::compile_world_record(&instance).unwrap()),
            },
        ],
        start_area as u32,
    )
}

#[test]
#[ignore = "requires approved DATs and accepted full native pack"]
fn approved_dat_nearby_academy_shop_placement() {
    let (records, area) = nearby_academy_shop_records();
    assert_eq!(area, 1);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].key.namespace, 2);
    assert_eq!(records[1].key.namespace, 20);
}

#[tokio::test]
#[ignore = "requires PostgreSQL, approved DATs, and accepted full native pack"]
async fn authenticated_nearby_academy_shop_use_fails_closed_without_durable_source() {
    use bace_wire::opcode::{GameActionType, GameEventType, GameMessageOpcode};
    const VENDOR: u32 = 0x7f00_a001;
    let (records, area) = nearby_academy_shop_records();
    let mut fixture =
        crate::game_runtime::tests::entered::fixture_with_start_area(records, area).await;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            fixture
                .runtime
                .poll(fixture.runtime.clock.monotonic.elapsed())
                .unwrap();
            fixture.client.drain(&fixture.runtime);
            if fixture.client.has_create(VENDOR, 0)
                && fixture
                    .runtime
                    .npc
                    .vendor_registration(EntityId(VENDOR))
                    .is_some()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("source-authored nearby Shop reaches authenticated visibility and NPC admission");

    let output_start = fixture.client.messages.len();
    let packet = GameActionEnvelope {
        sequence: 2,
        action: GameActionType::Use,
        payload: &VENDOR.to_le_bytes(),
    }
    .encode(4096)
    .unwrap();
    fixture
        .client
        .send_message(&fixture.runtime, fixture.key, &packet);
    let mut reached_cold = false;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            fixture
                .runtime
                .poll(fixture.runtime.clock.monotonic.elapsed())
                .unwrap();
            fixture.client.drain(&fixture.runtime);
            if let Some(pending) = fixture.runtime.vendors.pending.as_ref() {
                reached_cold |= matches!(&pending.phase, Phase::Cold(_));
            }
            if fixture
                .runtime
                .sessions
                .get(&fixture.key)
                .and_then(|session| session.failure.as_deref())
                == Some("vendor has no durable world source")
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("missing durable world source produces an explicit retained failure");
    assert!(
        reached_cold,
        "authenticated Use reached the vendor's cold owner"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !fixture.runtime.sessions[&fixture.key].terminated {
            fixture
                .runtime
                .poll(fixture.runtime.clock.monotonic.elapsed())
                .unwrap();
            fixture.client.drain(&fixture.runtime);
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("missing source closes the authenticated Shop Use without success");
    assert!(
        fixture.runtime.sessions[&fixture.key].terminated,
        "rejected unsupported Shop cannot publish a success"
    );
    assert!(
        !fixture.client.messages[output_start..]
            .iter()
            .any(|message| {
                message.queue == 9
                    && message
                        .bytes
                        .starts_with(&GameMessageOpcode::GameEvent.0.to_le_bytes())
                    && message.bytes.get(12..16)
                        == Some(GameEventType::ApproachVendor.0.to_le_bytes().as_slice())
                    && message.bytes.get(16..20) == Some(VENDOR.to_le_bytes().as_slice())
            }),
        "unsupported Shop cannot fabricate an ApproachVendor listing"
    );
    let source = fixture
        .runtime
        .bootstrap
        .store
        .load_vendor_state(VENDOR)
        .await
        .unwrap();
    assert!(source.is_none(), "static Shop lacks a V5 world source");
}
