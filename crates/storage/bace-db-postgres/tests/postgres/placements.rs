use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::*;
use bace_storage_codec::*;
fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "placement_fixture".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
async fn player(store: &PgStore, n: u32) -> (PlayerSaveV1, CharacterLease) {
    let CreateAccountOutcome::Created(a) = store
        .create(NewAccount {
            name: AccountName::parse(&format!("placement{n}")).unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    let p = PlayerSaveV1 {
        entity: entity(store.allocate_player_id().await.unwrap()),
        account_id: a.id.0,
        name: format!("Player {n}"),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&p, 0, 11, &[]).await.unwrap();
    (p, lease)
}
fn snapshot(e: &EntitySaveV1, version: i64, bytes: Vec<u8>) -> SaveSnapshot {
    SaveSnapshot {
        object_id: e.object_id,
        mutation_revision: e.mutation_revision,
        expected_version: version,
        bytes,
    }
}
fn position() -> bace_content::Position {
    bace_content::Position {
        obj_cell_id: 0x12340001,
        position_x: 1.0,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    }
}
fn contained(id: u32) -> ItemPlacementV2 {
    ItemPlacementV2::Contained {
        container: id,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    }
}
fn place(id: u32) -> DurableItemPlace {
    DurableItemPlace::Contained {
        container: id,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    }
}
#[tokio::test]
async fn zero_drop_no_corpse_has_an_exact_unplaced_death_receipt() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let event_id = [7; 16];
    let marker = PveDeathReceiptV1 {
        marker_object_id: 0x8000_0d01,
        event_id,
        victim_object_id: 0x5000_0d01,
        position: position(),
        world_epoch: owner.epoch(),
        killed: true,
    };
    let bytes = marker.encode().unwrap();
    let snapshot = SaveSnapshot {
        object_id: marker.marker_object_id,
        expected_version: 0,
        mutation_revision: 1,
        bytes: bytes.clone(),
    };
    let operation = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: format!("native-no-corpse-{:032x}", u128::from_le_bytes(event_id)),
            snapshots: vec![snapshot.clone()],
            participants: vec![marker.marker_object_id],
            leases: vec![],
            changes: vec![],
            storage_views: vec![],
        },
    };
    assert!(matches!(
        store.world_placement_operation(&operation).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store
            .load(marker.marker_object_id)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        bytes
    );
    assert_eq!(
        store.item_place(marker.marker_object_id).await.unwrap(),
        None
    );
    assert_eq!(
        store.world_placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    assert!(store.save_batch(&[snapshot]).await.is_err());
    let mut changed = operation.clone();
    changed.inventory.snapshots[0].bytes = PveDeathReceiptV1 {
        victim_object_id: 0x5000_0d02,
        ..marker
    }
    .encode()
    .unwrap();
    assert!(matches!(
        store.world_placement_operation(&changed).await,
        Err(StoreError::OperationMismatch)
    ));
    owner.close().await.unwrap();
    store.close().await;
}
#[tokio::test]
async fn world_drop_pickup_race_receipts_and_reload_are_atomic() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let (alice, a) = player(&store, 1).await;
    let (bob, b) = player(&store, 2).await;
    let item =
        ItemSaveV2::migrate_v1(entity(0x80000001), ItemPlacementV2::World(position())).unwrap();
    let create = PlacementOperation {
        operation_id: "drop-new".into(),
        snapshots: vec![snapshot(&item.entity, 0, item.encode().unwrap())],
        participants: vec![item.entity.object_id],
        leases: vec![],
        changes: vec![PlacementChange {
            item: item.entity.object_id,
            expected: None,
            destination: DurableItemPlace::World { cell: 0x12340001 },
        }],
        storage_views: vec![],
    };
    store.placement_operation(&create).await.unwrap();
    assert_eq!(
        store.placement_operation(&create).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    assert_eq!(
        store
            .load_world_items(0x12340001, 10, 10000)
            .await
            .unwrap()
            .len(),
        1
    );
    let pickup = |p: &PlayerSaveV1, lease: CharacterLease, op: &str| {
        let mut i = item.clone();
        i.entity.mutation_revision = 2;
        i.placement = contained(p.entity.object_id);
        PlacementOperation {
            operation_id: op.into(),
            snapshots: vec![snapshot(&i.entity, 1, i.encode().unwrap())],
            participants: vec![p.entity.object_id, i.entity.object_id],
            leases: vec![lease],
            changes: vec![PlacementChange {
                item: i.entity.object_id,
                expected: Some(DurableItemPlace::World { cell: 0x12340001 }),
                destination: place(p.entity.object_id),
            }],
            storage_views: vec![],
        }
    };
    let first = pickup(&alice, a, "pickup-a");
    let second = pickup(&bob, b, "pickup-b");
    let (one, two) = tokio::join!(
        store.placement_operation(&first),
        store.placement_operation(&second)
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let (winner, loser) = if one.is_ok() {
        (a, "pickup-b")
    } else {
        (b, "pickup-a")
    };
    assert!(store.resolve_operation(loser).await.unwrap().is_none());
    assert!(
        store
            .load_world_items(0x12340001, 10, 10000)
            .await
            .unwrap()
            .is_empty()
    );
    let loading = store.begin_login(winner).await.unwrap();
    let inventory = store
        .load_character_inventory(loading.lease, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(inventory.items.len(), 1);
    assert_eq!(
        ItemSaveV2::decode(&inventory.items[0].aggregate.bytes)
            .unwrap()
            .placement,
        contained(winner.character_id)
    );
    assert!(matches!(
        store
            .save_batch(&[snapshot(&item.entity, 2, item.encode().unwrap())])
            .await,
        Err(StoreError::OwnershipConflict)
    ));
    store.close().await;
}
#[tokio::test]
async fn eviction_keeps_contents_denies_old_views_and_new_owner_inherits() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (alice, a) = player(&store, 1).await;
    let (bob, b) = player(&store, 2).await;
    let old = HouseSaveV1 {
        entity: entity(0x70000001),
        house_id: 42,
        owner_id: alice.entity.object_id,
        purchased_at: 1,
        rent_period_start: 1,
        rent_due_at: 2,
        access: vec![],
    };
    store.save_house("house-create", a, &old, 0).await.unwrap();
    let mut house = HouseSaveV2::migrate_v1(old).unwrap();
    let item =
        ItemSaveV2::migrate_v1(entity(0x80000001), contained(house.entity.object_id)).unwrap();
    let stash = PlacementOperation {
        operation_id: "stash".into(),
        snapshots: vec![snapshot(&item.entity, 0, item.encode().unwrap())],
        participants: vec![
            alice.entity.object_id,
            house.entity.object_id,
            item.entity.object_id,
        ],
        leases: vec![a],
        changes: vec![PlacementChange {
            item: item.entity.object_id,
            expected: None,
            destination: place(house.entity.object_id),
        }],
        storage_views: vec![StorageViewFence {
            actor: alice.entity.object_id,
            house: house.entity.object_id,
            generation: 1,
        }],
    };
    store.placement_operation(&stash).await.unwrap();
    house.entity.mutation_revision = 2;
    house.owner_id = None;
    house.access_generation = 2;
    let evict = HousingOperation {
        inventory: PlacementOperation {
            operation_id: "evict".into(),
            snapshots: vec![snapshot(&house.entity, 1, house.encode().unwrap())],
            participants: vec![alice.entity.object_id, house.entity.object_id],
            leases: vec![a],
            changes: vec![],
            storage_views: vec![],
        },
        ownership: HouseOwnershipChange {
            house: house.entity.object_id,
            house_id: 42,
            expected_owner: Some(alice.entity.object_id),
            expected_generation: 1,
            owner: None,
            generation: 2,
        },
    };
    store.housing_operation(&evict).await.unwrap();
    assert_eq!(
        store.item_place(item.entity.object_id).await.unwrap(),
        Some(place(house.entity.object_id))
    );
    let mut take = item.clone();
    take.entity.mutation_revision = 2;
    take.placement = contained(alice.entity.object_id);
    let stale = PlacementOperation {
        operation_id: "stale-view".into(),
        snapshots: vec![snapshot(&take.entity, 1, take.encode().unwrap())],
        participants: vec![
            alice.entity.object_id,
            house.entity.object_id,
            item.entity.object_id,
        ],
        leases: vec![a],
        changes: vec![PlacementChange {
            item: item.entity.object_id,
            expected: Some(place(house.entity.object_id)),
            destination: place(alice.entity.object_id),
        }],
        storage_views: vec![StorageViewFence {
            actor: alice.entity.object_id,
            house: house.entity.object_id,
            generation: 1,
        }],
    };
    assert!(store.placement_operation(&stale).await.is_err());
    assert!(
        store
            .resolve_operation("stale-view")
            .await
            .unwrap()
            .is_none()
    );
    house.entity.mutation_revision = 3;
    house.owner_id = Some(bob.entity.object_id);
    house.access_generation = 3;
    let purchase = HousingOperation {
        inventory: PlacementOperation {
            operation_id: "new-owner".into(),
            snapshots: vec![snapshot(&house.entity, 2, house.encode().unwrap())],
            participants: vec![bob.entity.object_id, house.entity.object_id],
            leases: vec![b],
            changes: vec![],
            storage_views: vec![],
        },
        ownership: HouseOwnershipChange {
            house: house.entity.object_id,
            house_id: 42,
            expected_owner: None,
            expected_generation: 2,
            owner: Some(bob.entity.object_id),
            generation: 3,
        },
    };
    store.housing_operation(&purchase).await.unwrap();
    let mut inherit = stale;
    inherit.operation_id = "inherit".into();
    inherit.participants[0] = bob.entity.object_id;
    inherit.leases = vec![b];
    inherit.storage_views[0].generation = 3;
    inherit.storage_views[0].actor = bob.entity.object_id;
    inherit.changes[0].destination = place(bob.entity.object_id);
    take.placement = contained(bob.entity.object_id);
    inherit.snapshots[0].bytes = take.encode().unwrap();
    store.placement_operation(&inherit).await.unwrap();
    assert_eq!(
        store.item_place(item.entity.object_id).await.unwrap(),
        Some(place(bob.entity.object_id))
    );
    store.close().await;
}
#[tokio::test]
async fn key_fingerprint_is_immutable_and_idempotent() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 3).await.unwrap();
    store.migrate().await.unwrap();
    store.bind_random_key(1, [7; 32]).await.unwrap();
    store.bind_random_key(1, [7; 32]).await.unwrap();
    assert!(store.bind_random_key(1, [8; 32]).await.is_err());
    assert!(store.bind_random_key(0, [7; 32]).await.is_err());
    store.bind_random_key(2, [8; 32]).await.unwrap();
    store.close().await;
}
#[tokio::test]
async fn registered_v2_snapshots_cannot_be_downgraded_by_legacy_writers() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, offline) = player(&store, 9).await;
    let mut v2 = PlayerSaveV2::migrate_v1(player.clone()).unwrap();
    v2.player.entity.mutation_revision = 2;
    let encoded = v2.encode().unwrap();
    store
        .save_offline(offline, &snapshot(&v2.player.entity, 1, encoded.clone()))
        .await
        .unwrap();
    let old = snapshot(&player.entity, 2, player.encode().unwrap());
    assert!(store.save_offline(offline, &old).await.is_err());
    let bad = InventoryOperation {
        operation_id: "forbidden-downgrade".into(),
        snapshots: vec![old.clone()],
        leases: vec![offline],
        transfers: vec![],
    };
    assert!(store.inventory_operation(&bad).await.is_err());
    assert!(
        store
            .resolve_operation("forbidden-downgrade")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .load(player.entity.object_id)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        encoded
    );
    let loaded = store.begin_login(offline).await.unwrap();
    let online = store.finish_login(loaded.lease).await.unwrap();
    assert!(store.save_owned(online, &old).await.is_err());
    assert!(
        store
            .save_owned_batch(&OwnedSaveBatch {
                snapshots: vec![old],
                participants: vec![player.entity.object_id],
                leases: vec![online]
            })
            .await
            .is_err()
    );
    let original = HouseSaveV1 {
        entity: entity(0x70000010),
        house_id: 90,
        owner_id: player.entity.object_id,
        purchased_at: 1,
        rent_period_start: 1,
        rent_due_at: 2,
        access: vec![],
    };
    store
        .save_house("house-legacy", online, &original, 0)
        .await
        .unwrap();
    let mut house = HouseSaveV2::migrate_v1(original.clone()).unwrap();
    house.entity.mutation_revision = 2;
    let operation = HousingOperation {
        inventory: PlacementOperation {
            operation_id: "house-upgrade".into(),
            snapshots: vec![snapshot(&house.entity, 1, house.encode().unwrap())],
            participants: vec![player.entity.object_id, house.entity.object_id],
            leases: vec![online],
            changes: vec![],
            storage_views: vec![],
        },
        ownership: HouseOwnershipChange {
            house: house.entity.object_id,
            house_id: 90,
            expected_owner: Some(player.entity.object_id),
            expected_generation: 1,
            owner: Some(player.entity.object_id),
            generation: 1,
        },
    };
    store.housing_operation(&operation).await.unwrap();
    assert!(
        store
            .save_house("house-downgrade", online, &original, 2)
            .await
            .is_err()
    );
    assert!(
        store
            .resolve_operation("house-downgrade")
            .await
            .unwrap()
            .is_none()
    );
    store.close().await;
}
#[tokio::test]
async fn corpse_tree_reload_preserves_receipt_and_cannot_delete_nonempty_contents() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 3).await.unwrap();
    store.migrate().await.unwrap();
    let corpse = CorpseSaveV2::migrate_v1(
        CorpseSaveV1 {
            entity: entity(0x80000200),
            owner: None,
            death_operation: "world-epoch/death-1".into(),
            expires_at: 100,
        },
        ItemPlacementV2::World(position()),
    )
    .unwrap();
    let child = ItemSaveV2::migrate_v1(
        entity(0x80000001),
        contained(corpse.corpse.entity.object_id),
    )
    .unwrap();
    let spawn = PlacementOperation {
        operation_id: "corpse-spawn".into(),
        snapshots: vec![
            snapshot(&corpse.corpse.entity, 0, corpse.encode().unwrap()),
            snapshot(&child.entity, 0, child.encode().unwrap()),
        ],
        participants: vec![corpse.corpse.entity.object_id, child.entity.object_id],
        leases: vec![],
        changes: vec![
            PlacementChange {
                item: corpse.corpse.entity.object_id,
                expected: None,
                destination: DurableItemPlace::World {
                    cell: position().obj_cell_id,
                },
            },
            PlacementChange {
                item: child.entity.object_id,
                expected: None,
                destination: place(corpse.corpse.entity.object_id),
            },
        ],
        storage_views: vec![],
    };
    store.placement_operation(&spawn).await.unwrap();
    let tree = store
        .load_world_item_tree(position().obj_cell_id, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].depth, 0);
    assert_eq!(tree[1].depth, 1);
    assert_eq!(
        CorpseSaveV2::decode(&tree[0].aggregate.bytes)
            .unwrap()
            .corpse
            .death_operation,
        "world-epoch/death-1"
    );
    let mut removed = corpse.clone();
    removed.corpse.entity.mutation_revision = 2;
    removed.placement = ItemPlacementV2::Removed;
    let cleanup = PlacementOperation {
        operation_id: "corpse-cleanup".into(),
        snapshots: vec![snapshot(
            &removed.corpse.entity,
            1,
            removed.encode().unwrap(),
        )],
        participants: vec![corpse.corpse.entity.object_id],
        leases: vec![],
        changes: vec![PlacementChange {
            item: corpse.corpse.entity.object_id,
            expected: Some(DurableItemPlace::World {
                cell: position().obj_cell_id,
            }),
            destination: DurableItemPlace::Removed,
        }],
        storage_views: vec![],
    };
    assert!(store.placement_operation(&cleanup).await.is_err());
    assert!(
        store
            .resolve_operation("corpse-cleanup")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .load_world_item_tree(position().obj_cell_id, InventoryLoadLimits::default())
            .await
            .unwrap()
            .len(),
        2
    );
    let mut removed_child = child.clone();
    removed_child.entity.mutation_revision = 2;
    removed_child.placement = ItemPlacementV2::Removed;
    store
        .placement_operation(&PlacementOperation {
            operation_id: "corpse-item-consume".into(),
            snapshots: vec![snapshot(
                &removed_child.entity,
                1,
                removed_child.encode().unwrap(),
            )],
            participants: vec![corpse.corpse.entity.object_id, child.entity.object_id],
            leases: vec![],
            changes: vec![PlacementChange {
                item: child.entity.object_id,
                expected: Some(place(corpse.corpse.entity.object_id)),
                destination: DurableItemPlace::Removed,
            }],
            storage_views: vec![],
        })
        .await
        .unwrap();
    store.placement_operation(&cleanup).await.unwrap();
    assert!(
        store
            .load_world_item_tree(position().obj_cell_id, InventoryLoadLimits::default())
            .await
            .unwrap()
            .is_empty()
    );
    store.close().await;
}
#[tokio::test]
async fn rare_identity_and_ordinals_cannot_be_erased_reseeded_or_rewound() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, lease) = player(&store, 33).await;
    let mut v2 = PlayerSaveV2::migrate_v1(player).unwrap();
    v2.player.entity.mutation_revision = 2;
    v2.rares = Some(RareStateV1 {
        character: lease.character_id,
        random_identity: [7; 16],
        key_version: 1,
        attempt_ordinal: 10,
        timer_ordinal: 3,
        next_realtime_at: Some(1000),
        last_effective_time: 99,
    });
    let first = snapshot(&v2.player.entity, 1, v2.encode().unwrap());
    assert!(store.save_offline(lease, &first).await.is_err());
    store.bind_random_key(1, [9; 32]).await.unwrap();
    store.save_offline(lease, &first).await.unwrap();
    for change in 0..6 {
        let mut next = v2.clone();
        next.player.entity.mutation_revision = 3;
        match change {
            0 => next.rares = None,
            1 => next.rares.as_mut().unwrap().random_identity = [8; 16],
            2 => next.rares.as_mut().unwrap().key_version = 2,
            3 => next.rares.as_mut().unwrap().attempt_ordinal = 9,
            4 => next.rares.as_mut().unwrap().timer_ordinal = 2,
            _ => next.rares.as_mut().unwrap().last_effective_time = 98,
        };
        let operation = InventoryOperation {
            operation_id: format!("bad-rare-{change}"),
            snapshots: vec![snapshot(&next.player.entity, 2, next.encode().unwrap())],
            leases: vec![lease],
            transfers: vec![],
        };
        assert!(store.inventory_operation(&operation).await.is_err());
        assert!(
            store
                .resolve_operation(&operation.operation_id)
                .await
                .unwrap()
                .is_none()
        );
    }
    let mut advanced = v2.clone();
    advanced.player.entity.mutation_revision = 3;
    advanced.rares.as_mut().unwrap().attempt_ordinal += 1;
    store
        .save_offline(
            lease,
            &snapshot(&advanced.player.entity, 2, advanced.encode().unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(
        PlayerSaveV2::decode(&store.load(lease.character_id).await.unwrap().unwrap().bytes)
            .unwrap()
            .rares
            .unwrap()
            .attempt_ordinal,
        11
    );
    store.close().await;
}

#[tokio::test]
async fn generated_acquisition_is_atomic_epoch_fenced_and_exactly_replayable() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let (alice, lease) = player(&store, 31).await;
    let item =
        ItemSaveV2::migrate_v1(entity(0x80000031), contained(alice.entity.object_id)).unwrap();
    let operation = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "generated-acquisition".into(),
            snapshots: vec![snapshot(&item.entity, 0, item.encode().unwrap())],
            participants: vec![alice.entity.object_id, item.entity.object_id],
            leases: vec![lease],
            changes: vec![PlacementChange {
                item: item.entity.object_id,
                expected: None,
                destination: place(alice.entity.object_id),
            }],
            storage_views: vec![],
        },
    };
    assert!(matches!(
        store.world_placement_operation(&operation).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.item_place(item.entity.object_id).await.unwrap(),
        Some(place(alice.entity.object_id))
    );
    owner.close().await.unwrap();
    let next_owner = store.acquire_world_owner().await.unwrap();
    assert!(next_owner.epoch() > operation.world_epoch);
    // An uncertain old commit resolves under its original immutable identity.
    assert_eq!(
        store.world_placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut changed = operation.clone();
    changed.world_epoch = next_owner.epoch();
    assert!(store.world_placement_operation(&changed).await.is_err());
    let mut stale = operation.clone();
    stale.inventory.operation_id = "stale-generated-acquisition".into();
    assert!(store.world_placement_operation(&stale).await.is_err());
    assert!(
        store
            .resolve_operation(&stale.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.item_place(item.entity.object_id).await.unwrap(),
        Some(place(alice.entity.object_id))
    );
    next_owner.close().await.unwrap();
    store.close().await;
}

#[tokio::test]
async fn generated_world_remainder_retirement_is_atomic_and_replays_after_epoch_restart() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 6).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let (alice, offline) = player(&store, 32).await;
    let loading = store.begin_login(offline).await.unwrap();
    let online = store.finish_login(loading.lease).await.unwrap();
    let world = DurableItemPlace::World {
        cell: position().obj_cell_id,
    };
    let make_item = |id, stack, placement| {
        let mut item = entity(id);
        item.state.weenie_type = 51;
        item.state.properties.ints = [
            (5, stack * 2),
            (11, 100),
            (12, stack),
            (13, 2),
            (15, 3),
            (19, stack * 3),
        ]
        .into_iter()
        .map(|(id, value)| bace_content::Property { id, value })
        .collect();
        ItemSaveV3::migrate_v2(ItemSaveV2::migrate_v1(item, placement).unwrap()).unwrap()
    };
    // First durable acquisition publishes the picked-up portion and both world
    // remainders together. The smaller-ID companion exercises rollback after a
    // valid earlier CAS write when the later remainder CAS is stale.
    let companion = make_item(0x80001001, 2, ItemPlacementV2::World(position()));
    let mut remainder = make_item(0x80001002, 7, ItemPlacementV2::World(position()));
    let acquired = make_item(0x80001003, 3, contained(alice.entity.object_id));
    let acquisition = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "generated-retirement/acquire".into(),
            snapshots: [&companion, &remainder, &acquired]
                .into_iter()
                .map(|item| snapshot(&item.entity, 0, item.encode().unwrap()))
                .collect(),
            participants: vec![
                alice.entity.object_id,
                companion.entity.object_id,
                remainder.entity.object_id,
                acquired.entity.object_id,
            ],
            leases: vec![online],
            changes: [&companion, &remainder, &acquired]
                .into_iter()
                .map(|item| PlacementChange {
                    item: item.entity.object_id,
                    expected: None,
                    destination: if item.entity.object_id == acquired.entity.object_id {
                        place(alice.entity.object_id)
                    } else {
                        world
                    },
                })
                .collect(),
            storage_views: vec![],
        },
    };
    assert!(matches!(
        store.world_placement_operation(&acquisition).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.item_place(acquired.entity.object_id).await.unwrap(),
        Some(place(alice.entity.object_id))
    );

    // A newer accepted remainder revision must defeat an older retirement.
    remainder.entity.mutation_revision = 2;
    remainder
        .entity
        .state
        .properties
        .ints
        .iter_mut()
        .find(|p| p.id == 12)
        .unwrap()
        .value = 6;
    remainder
        .entity
        .state
        .properties
        .ints
        .iter_mut()
        .find(|p| p.id == 5)
        .unwrap()
        .value = 12;
    remainder
        .entity
        .state
        .properties
        .ints
        .iter_mut()
        .find(|p| p.id == 19)
        .unwrap()
        .value = 18;
    let advance = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "generated-retirement/advance".into(),
            snapshots: vec![snapshot(&remainder.entity, 1, remainder.encode().unwrap())],
            participants: vec![remainder.entity.object_id],
            leases: vec![],
            changes: vec![PlacementChange {
                item: remainder.entity.object_id,
                expected: Some(world),
                destination: world,
            }],
            storage_views: vec![],
        },
    };
    store.world_placement_operation(&advance).await.unwrap();
    let mut removed_companion = companion.clone();
    removed_companion.entity.mutation_revision = 2;
    removed_companion.previous.placement = ItemPlacementV2::Removed;
    let mut removed_remainder = remainder.clone();
    removed_remainder.entity.mutation_revision = 3;
    removed_remainder.previous.placement = ItemPlacementV2::Removed;
    let mut retirement = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "generated-retirement/stale".into(),
            snapshots: vec![
                snapshot(
                    &removed_companion.entity,
                    1,
                    removed_companion.encode().unwrap(),
                ),
                snapshot(
                    &removed_remainder.entity,
                    1,
                    removed_remainder.encode().unwrap(),
                ),
            ],
            participants: vec![companion.entity.object_id, remainder.entity.object_id],
            leases: vec![],
            changes: [&companion, &remainder]
                .into_iter()
                .map(|item| PlacementChange {
                    item: item.entity.object_id,
                    expected: Some(world),
                    destination: DurableItemPlace::Removed,
                })
                .collect(),
            storage_views: vec![],
        },
    };
    assert!(
        matches!(store.world_placement_operation(&retirement).await, Err(StoreError::Conflict(id)) if id == remainder.entity.object_id)
    );
    assert!(
        store
            .resolve_operation(&retirement.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    for (item, version) in [(&companion, 1), (&remainder, 2)] {
        assert_eq!(
            store.item_place(item.entity.object_id).await.unwrap(),
            Some(world)
        );
        let saved = store.load(item.entity.object_id).await.unwrap().unwrap();
        assert_eq!(saved.persisted_version, version);
        assert_eq!(saved.bytes, item.encode().unwrap());
    }
    assert_eq!(
        store
            .load_world_items(position().obj_cell_id, 10, 100_000)
            .await
            .unwrap()
            .len(),
        2
    );

    retirement.inventory.operation_id = "generated-retirement/commit".into();
    retirement.inventory.snapshots[1].expected_version = 2;
    assert!(retirement.inventory.leases.is_empty());
    assert_eq!(
        store.world_placement_operation(&retirement).await.unwrap(),
        OperationOutcome::Committed(vec![
            SaveAck {
                object_id: companion.entity.object_id,
                mutation_revision: 2,
                persisted_version: 2
            },
            SaveAck {
                object_id: remainder.entity.object_id,
                mutation_revision: 3,
                persisted_version: 3
            },
        ])
    );
    for (item, version) in [(&removed_companion, 2), (&removed_remainder, 3)] {
        assert_eq!(
            store.item_place(item.entity.object_id).await.unwrap(),
            Some(DurableItemPlace::Removed)
        );
        let saved = store.load(item.entity.object_id).await.unwrap().unwrap();
        assert_eq!(saved.persisted_version, version);
        assert_eq!(saved.bytes, item.encode().unwrap());
    }
    assert!(
        store
            .load_world_items(position().obj_cell_id, 10, 100_000)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .load(acquired.entity.object_id)
            .await
            .unwrap()
            .unwrap()
            .bytes,
        acquired.encode().unwrap()
    );
    owner.close().await.unwrap();
    let next_owner = store.acquire_world_owner().await.unwrap();
    assert!(next_owner.epoch() > retirement.world_epoch);
    assert_eq!(
        store.world_placement_operation(&retirement).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut changed_epoch = retirement.clone();
    changed_epoch.world_epoch = next_owner.epoch();
    assert!(matches!(
        store.world_placement_operation(&changed_epoch).await,
        Err(StoreError::OperationMismatch)
    ));
    let mut stale_epoch = retirement.clone();
    stale_epoch.inventory.operation_id = "generated-retirement/old-epoch".into();
    assert!(matches!(
        store.world_placement_operation(&stale_epoch).await,
        Err(StoreError::OwnershipConflict)
    ));
    assert!(
        store
            .resolve_operation(&stale_epoch.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .load(remainder.entity.object_id)
            .await
            .unwrap()
            .unwrap()
            .persisted_version,
        3
    );
    assert_eq!(
        store.item_place(acquired.entity.object_id).await.unwrap(),
        Some(place(alice.entity.object_id))
    );
    next_owner.close().await.unwrap();
    store.close().await;
}
