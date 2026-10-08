use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{
    CharacterLease, DurableItemPlace, OperationOutcome, PlacementChange, PlacementOperation,
    SaveSnapshot, VendorStockOperation, VendorStockWrite, WorldPlacementOperation,
};
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV3, ItemSaveV4, ItemSaveV5, PlayerSaveV1,
    PlayerSaveV6, VendorDefaultStockV1, VendorStockSaveV1,
};

const VENDOR: u32 = 0x8000_3400;
const MARKER: u32 = 0x8000_3401;
const DEFAULT: u32 = 0x8000_3402;
const COIN: u32 = 0x8000_3403;
const GRANT: u32 = 0x8000_3404;

fn position() -> bace_content::Position {
    bace_content::Position {
        obj_cell_id: 0x1234_0100,
        position_x: 1.0,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    }
}

fn state(template: u32, kind: u32, values: &[(u32, i32)]) -> bace_content::WeenieV1 {
    bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: template,
        class_name: format!("vendor_buy_{template}"),
        weenie_type: kind,
        last_modified: None,
        properties: bace_content::SparseProperties {
            ints: values
                .iter()
                .map(|&(id, value)| bace_content::Property { id, value })
                .collect(),
            ..Default::default()
        },
    }
}

fn item(
    id: u32,
    template: u32,
    kind: u32,
    place: ItemPlacementV2,
    values: &[(u32, i32)],
) -> ItemSaveV5 {
    let entity = EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: state(template, kind, values),
    };
    let v2 = ItemSaveV2::migrate_v1(entity, place).unwrap();
    let v3 = ItemSaveV3::migrate_v2(v2).unwrap();
    let v4 = ItemSaveV4::migrate_v3(v3).unwrap();
    ItemSaveV5 {
        previous: v4,
        source_destination: (id == DEFAULT || id == GRANT).then_some(4),
    }
}

fn set_int(state: &mut bace_content::WeenieV1, id: u32, value: i32) {
    if let Some(entry) = state
        .properties
        .ints
        .iter_mut()
        .find(|entry| entry.id == id)
    {
        entry.value = value;
    } else {
        state
            .properties
            .ints
            .push(bace_content::Property { id, value });
        state.properties.ints.sort_by_key(|entry| entry.id);
    }
}

fn int(state: &bace_content::WeenieV1, id: u32) -> Option<i32> {
    state
        .properties
        .ints
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.value)
}

fn place(container: u32, slot: u32) -> ItemPlacementV2 {
    ItemPlacementV2::Contained {
        container,
        slot,
        pack_slot: false,
        equipped: 0,
    }
}

fn durable(container: u32, slot: u32) -> DurableItemPlace {
    DurableItemPlace::Contained {
        container,
        slot,
        pack_slot: false,
        equipped: 0,
    }
}

fn snapshot(id: u32, version: i64, revision: u64, bytes: Vec<u8>) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        expected_version: version,
        mutation_revision: revision,
        bytes,
    }
}

async fn player(store: &PgStore) -> (PlayerSaveV6, CharacterLease) {
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("vendorbuyer").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"fixture-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new test account")
    };
    let id = store.allocate_player_id().await.unwrap();
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: state(2, 10, &[(5, 0), (20, 0)]),
        },
        account_id: account.id.0,
        name: "Vendor Buyer".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&player, 0, 11, &[]).await.unwrap();
    let loading = store.begin_login(lease).await.unwrap();
    let online = store.finish_login(loading.lease).await.unwrap();
    (PlayerSaveV6::migrate_v1(player).unwrap(), online)
}

#[tokio::test]
async fn default_buy_commits_coin_player_vendor_grant_and_marker_as_one_receipt() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let (mut buyer, lease) = player(&store).await;
    let buyer_id = buyer.player.entity.object_id;

    let vendor = item(VENDOR, 3, 12, ItemPlacementV2::World(position()), &[]);
    store
        .world_placement_operation(&WorldPlacementOperation {
            world_epoch: owner.epoch(),
            inventory: PlacementOperation {
                operation_id: "vendor-buy-create-vendor".into(),
                snapshots: vec![snapshot(VENDOR, 0, 1, vendor.encode().unwrap())],
                participants: vec![VENDOR],
                leases: vec![],
                changes: vec![PlacementChange {
                    item: VENDOR,
                    expected: None,
                    destination: DurableItemPlace::World {
                        cell: position().obj_cell_id,
                    },
                }],
                storage_views: vec![],
            },
        })
        .await
        .unwrap();
    let default = item(DEFAULT, 4, 1, place(VENDOR, 0), &[(5, 1), (19, 10)]);
    let marker = VendorStockSaveV1 {
        marker_object_id: MARKER,
        vendor_object_id: VENDOR,
        source_revision: 1,
        source_hash: [7; 32],
        loaded: true,
        stock_revision: 2,
        defaults: vec![VendorDefaultStockV1 {
            root: DEFAULT,
            display_quantity: -1,
            contribution_units: 0,
            child_ids: vec![],
        }],
        unique: vec![],
    };
    store
        .vendor_stock_operation(&VendorStockOperation {
            world_epoch: owner.epoch(),
            vendor_expected_version: 1,
            inventory: PlacementOperation {
                operation_id: "vendor-buy-load-stock".into(),
                snapshots: vec![snapshot(DEFAULT, 0, 1, default.encode().unwrap())],
                participants: vec![VENDOR, MARKER, DEFAULT],
                leases: vec![],
                changes: vec![PlacementChange {
                    item: DEFAULT,
                    expected: None,
                    destination: durable(VENDOR, 0),
                }],
                storage_views: vec![],
            },
            marker: VendorStockWrite {
                marker_object_id: MARKER,
                expected_version: 0,
                expected_stock_revision: 0,
                mutation_revision: 1,
                bytes: marker.encode().unwrap(),
            },
        })
        .await
        .unwrap();

    let coin = item(
        COIN,
        273,
        14,
        place(buyer_id, 0),
        &[(5, 100), (12, 100), (19, 100)],
    );
    buyer.player.entity.mutation_revision = 2;
    set_int(&mut buyer.player.entity.state, 5, 100);
    set_int(&mut buyer.player.entity.state, 20, 100);
    store
        .world_placement_operation(&WorldPlacementOperation {
            world_epoch: owner.epoch(),
            inventory: PlacementOperation {
                operation_id: "vendor-buy-fund-player".into(),
                snapshots: vec![
                    snapshot(buyer_id, 1, 2, buyer.encode().unwrap()),
                    snapshot(COIN, 0, 1, coin.encode().unwrap()),
                ],
                participants: vec![buyer_id, COIN],
                leases: vec![lease],
                changes: vec![PlacementChange {
                    item: COIN,
                    expected: None,
                    destination: durable(buyer_id, 0),
                }],
                storage_views: vec![],
            },
        })
        .await
        .unwrap();
    let before = store.load_vendor_state(VENDOR).await.unwrap().unwrap();
    assert_eq!(before.source.aggregate.persisted_version, 1);
    assert_eq!(before.forest.as_ref().unwrap().marker.persisted_version, 1);
    assert_eq!(
        VendorStockSaveV1::decode(&before.forest.as_ref().unwrap().marker.bytes)
            .unwrap()
            .stock_revision,
        2
    );

    let mut spent_coin = coin.clone();
    spent_coin.entity.mutation_revision = 2;
    for id in [5, 12, 19] {
        set_int(&mut spent_coin.entity.state, id, 90);
    }
    let mut paid_player = buyer.clone();
    paid_player.player.entity.mutation_revision = 3;
    set_int(&mut paid_player.player.entity.state, 5, 91);
    set_int(&mut paid_player.player.entity.state, 20, 90);
    let mut credited_vendor = vendor.clone();
    credited_vendor.entity.mutation_revision = 2;
    set_int(&mut credited_vendor.entity.state, 77, 1);
    set_int(&mut credited_vendor.entity.state, 79, 10);
    let grant = item(GRANT, 4, 1, place(buyer_id, 1), &[(5, 1), (19, 10)]);
    let buy_marker = VendorStockSaveV1 {
        stock_revision: 3,
        ..marker.clone()
    };
    let buy = VendorStockOperation {
        world_epoch: owner.epoch(),
        vendor_expected_version: 1,
        inventory: PlacementOperation {
            operation_id: "vendor-default-buy".into(),
            snapshots: vec![
                snapshot(buyer_id, 2, 3, paid_player.encode().unwrap()),
                snapshot(VENDOR, 1, 2, credited_vendor.encode().unwrap()),
                snapshot(COIN, 1, 2, spent_coin.encode().unwrap()),
                snapshot(GRANT, 0, 1, grant.encode().unwrap()),
            ],
            participants: vec![buyer_id, VENDOR, MARKER, DEFAULT, COIN, GRANT],
            leases: vec![lease],
            changes: vec![
                PlacementChange {
                    item: COIN,
                    expected: Some(durable(buyer_id, 0)),
                    destination: durable(buyer_id, 0),
                },
                PlacementChange {
                    item: GRANT,
                    expected: None,
                    destination: durable(buyer_id, 1),
                },
            ],
            storage_views: vec![],
        },
        marker: VendorStockWrite {
            marker_object_id: MARKER,
            expected_version: 1,
            expected_stock_revision: 2,
            mutation_revision: 2,
            bytes: buy_marker.encode().unwrap(),
        },
    };
    let mut stale = buy.clone();
    stale.inventory.operation_id = "vendor-default-buy-stale".into();
    stale
        .inventory
        .snapshots
        .iter_mut()
        .find(|s| s.object_id == COIN)
        .unwrap()
        .expected_version = 0;
    assert!(matches!(
        store.vendor_stock_operation(&stale).await,
        Err(StoreError::Conflict(COIN))
    ));
    assert!(
        store
            .resolve_operation(&stale.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.load(buyer_id).await.unwrap().unwrap().bytes,
        buyer.encode().unwrap()
    );
    assert_eq!(
        store.load(VENDOR).await.unwrap().unwrap().bytes,
        vendor.encode().unwrap()
    );
    assert_eq!(
        store.load(COIN).await.unwrap().unwrap().bytes,
        coin.encode().unwrap()
    );
    assert!(store.load(GRANT).await.unwrap().is_none());
    assert_eq!(
        store.load_vendor_state(VENDOR).await.unwrap().unwrap(),
        before
    );

    let OperationOutcome::Committed(acks) = store.vendor_stock_operation(&buy).await.unwrap()
    else {
        panic!("purchase must commit")
    };
    assert_eq!(acks.len(), 5);
    for (id, version) in [
        (buyer_id, 3),
        (VENDOR, 2),
        (COIN, 2),
        (GRANT, 1),
        (MARKER, 2),
    ] {
        assert!(
            acks.iter()
                .any(|ack| ack.object_id == id && ack.persisted_version == version)
        );
    }
    assert_eq!(
        store.vendor_stock_operation(&buy).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut mismatch = buy.clone();
    mismatch
        .inventory
        .snapshots
        .iter_mut()
        .find(|s| s.object_id == GRANT)
        .unwrap()
        .bytes = default.encode().unwrap();
    assert!(matches!(
        store.vendor_stock_operation(&mismatch).await,
        Err(StoreError::OperationMismatch)
    ));
    let after = store.load_vendor_state(VENDOR).await.unwrap().unwrap();
    assert_eq!(after.source.aggregate.persisted_version, 2);
    assert_eq!(
        int(
            &ItemSaveV5::decode(&after.source.aggregate.bytes)
                .unwrap()
                .entity
                .state,
            79
        ),
        Some(10)
    );
    let forest = after.forest.unwrap();
    assert_eq!(forest.marker.persisted_version, 2);
    assert_eq!(
        VendorStockSaveV1::decode(&forest.marker.bytes)
            .unwrap()
            .stock_revision,
        3
    );
    assert_eq!(forest.items.len(), 1);
    assert_eq!(forest.items[0].aggregate.object_id, DEFAULT);
    assert_eq!(forest.items[0].aggregate.persisted_version, 1);
    assert_eq!(
        store.item_place(GRANT).await.unwrap(),
        Some(durable(buyer_id, 1))
    );
    let saved_player =
        PlayerSaveV6::decode(&store.load(buyer_id).await.unwrap().unwrap().bytes).unwrap();
    assert_eq!(int(&saved_player.player.entity.state, 20), Some(90));
    let saved_coin = ItemSaveV5::decode(&store.load(COIN).await.unwrap().unwrap().bytes).unwrap();
    assert_eq!(int(&saved_coin.entity.state, 12), Some(90));
    owner.close().await.unwrap();
    store.close().await;
}
