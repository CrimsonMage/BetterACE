//! A player Bool29 death moves a selected bag to the world while its children
//! retain exact contained placement in the same fenced placement receipt.
use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{
    DurableItemPlace, InventoryLoadLimits, OperationOutcome, PlacementChange, PlacementOperation,
    SaveSnapshot, WorldPlacementOperation,
};
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5, PlayerSaveV1, PlayerSaveV6,
};

fn state(id: u32, kind: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: id,
            class_name: format!("no_corpse_forest_{id}"),
            weenie_type: kind,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn snapshot(id: u32, revision: u64, version: i64, bytes: Vec<u8>) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        mutation_revision: revision,
        expected_version: version,
        bytes,
    }
}
fn position() -> bace_content::Position {
    bace_content::Position {
        obj_cell_id: 0x12340001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    }
}
fn item(id: u32, kind: u32, origin: u8, placement: ItemPlacementV2) -> ItemSaveV5 {
    ItemSaveV5 {
        previous: ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity: state(id, kind),
            placement,
        })
        .unwrap(),
        source_destination: Some(origin),
    }
}

#[tokio::test]
async fn selected_container_and_retained_child_commit_replay_or_roll_back_together() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("nocorpseforest").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let actor = store.allocate_player_id().await.unwrap();
    let player = PlayerSaveV1 {
        entity: state(actor, 10),
        account_id: account.id.0,
        name: "NoCorpse Forest".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&player, 0, 11, &[]).await.unwrap();
    let root = 0x8000_0121;
    let child = 0x8000_0122;
    let root_pack = ItemPlacementV2::Contained {
        container: actor,
        slot: 0,
        pack_slot: true,
        equipped: 0,
    };
    let child_in_root = ItemPlacementV2::Contained {
        container: root,
        slot: 2,
        pack_slot: true,
        equipped: 0,
    };
    let root_before = item(root, 20, 8, root_pack.clone());
    let child_before = item(child, 1, 1, child_in_root.clone());
    let root_contained = DurableItemPlace::Contained {
        container: actor,
        slot: 0,
        pack_slot: true,
        equipped: 0,
    };
    let child_contained = DurableItemPlace::Contained {
        container: root,
        slot: 2,
        pack_slot: true,
        equipped: 0,
    };
    let create = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "no-corpse-forest-initial".into(),
            snapshots: vec![
                snapshot(root, 1, 0, root_before.encode().unwrap()),
                snapshot(child, 1, 0, child_before.encode().unwrap()),
            ],
            participants: vec![actor, root, child],
            leases: vec![lease],
            changes: vec![
                PlacementChange {
                    item: root,
                    expected: None,
                    destination: root_contained,
                },
                PlacementChange {
                    item: child,
                    expected: None,
                    destination: child_contained,
                },
            ],
            storage_views: vec![],
        },
    };
    assert!(matches!(
        store.world_placement_operation(&create).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    let base = store.load(actor).await.unwrap().unwrap();
    let mut player_after = PlayerSaveV6::decode_or_migrate(&base.bytes).unwrap();
    player_after.player.entity.mutation_revision += 1;
    let mut root_after = root_before.clone();
    root_after.entity.mutation_revision += 1;
    root_after.placement = ItemPlacementV2::World(position());
    let mut child_after = child_before.clone();
    child_after.entity.mutation_revision += 1;
    let death = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: "player-no-corpse-container-forest".into(),
            snapshots: vec![
                snapshot(
                    actor,
                    player_after.player.entity.mutation_revision,
                    base.persisted_version,
                    player_after.encode().unwrap(),
                ),
                snapshot(root, 2, 1, root_after.encode().unwrap()),
                // The child keeps its exact parent/slot and receives one
                // revision touch in the same durable death receipt.
                snapshot(child, 2, 1, child_after.encode().unwrap()),
            ],
            participants: vec![actor, root, child],
            leases: vec![lease],
            changes: vec![PlacementChange {
                item: root,
                expected: Some(root_contained),
                destination: DurableItemPlace::World { cell: 0x12340001 },
            }],
            storage_views: vec![],
        },
    };
    let mut stale = death.clone();
    stale.inventory.operation_id.push_str("-stale");
    stale.inventory.snapshots[2].expected_version = 2;
    assert!(store.world_placement_operation(&stale).await.is_err());
    assert_eq!(store.item_place(root).await.unwrap(), Some(root_contained));
    assert_eq!(
        store.item_place(child).await.unwrap(),
        Some(child_contained)
    );
    assert_eq!(
        store.load(actor).await.unwrap().unwrap().persisted_version,
        1
    );
    assert!(
        store
            .resolve_operation(&stale.inventory.operation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        store.world_placement_operation(&death).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.world_placement_operation(&death).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let forest = store
        .load_world_item_tree(position().obj_cell_id, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(forest.len(), 2);
    assert_eq!(forest[0].aggregate.object_id, root);
    assert_eq!(
        forest[0].placement,
        DurableItemPlace::World { cell: 0x12340001 }
    );
    assert_eq!(forest[1].aggregate.object_id, child);
    assert_eq!(forest[1].placement, child_contained);
    assert_eq!(forest[1].aggregate.persisted_version, 2);
    assert_eq!(
        ItemSaveV5::decode(&forest[1].aggregate.bytes).unwrap(),
        child_after
    );
    let mut changed_replay = death.clone();
    changed_replay.inventory.snapshots[2].bytes = root_after.encode().unwrap();
    assert!(matches!(
        store.world_placement_operation(&changed_replay).await,
        Err(StoreError::OperationMismatch)
    ));
    owner.close().await.unwrap();
    store.close().await;
}
