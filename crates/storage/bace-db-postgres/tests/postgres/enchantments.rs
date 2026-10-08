use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::PgStore;
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
fn registry() -> Vec<FrozenEnchantmentV1> {
    vec![FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 7,
        spell_id: 123,
        layer_id: 3,
        has_spell_set_id: false,
        spell_category: 23,
        power_level: 100,
        start_time: -15.0,
        duration: 60.0,
        caster_object_id: 7,
        degrade_modifier: 0.1,
        degrade_limit: 0.5,
        last_time_degraded: -2.0,
        stat_mod_type: 0x1000,
        stat_mod_key: 7,
        stat_mod_value: -10.0,
        spell_set_id: 42,
    }]
}
#[tokio::test]
async fn world_corpse_and_item_v3_reload_transfer_and_downgrade_fences() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, lease) = player(&store, 73).await;
    let corpse = CorpseSaveV3 {
        previous: CorpseSaveV2::migrate_v1(
            CorpseSaveV1 {
                entity: entity(0x80000200),
                owner: None,
                death_operation: "v3-death".into(),
                expires_at: 100,
            },
            ItemPlacementV2::World(position()),
        )
        .unwrap(),
        enchantments: registry(),
    };
    let child = ItemSaveV3 {
        previous: ItemSaveV2::migrate_v1(
            entity(0x80000001),
            contained(corpse.corpse.entity.object_id),
        )
        .unwrap(),
        enchantments: registry(),
    };
    let operation = PlacementOperation {
        operation_id: "v3-corpse".into(),
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
    store.placement_operation(&operation).await.unwrap();
    let tree = store
        .load_world_item_tree(position().obj_cell_id, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(
        CorpseSaveV3::decode(&tree[0].aggregate.bytes).unwrap(),
        corpse
    );
    assert_eq!(ItemSaveV3::decode(&tree[1].aggregate.bytes).unwrap(), child);
    for (id, bytes) in [
        (corpse.corpse.entity.object_id, {
            let mut old = corpse.previous.clone();
            old.corpse.entity.mutation_revision = 2;
            old.encode().unwrap()
        }),
        (child.entity.object_id, {
            let mut old = child.previous.clone();
            old.entity.mutation_revision = 2;
            old.encode().unwrap()
        }),
    ] {
        let mut downgrade = operation.clone();
        downgrade.operation_id = format!("v3-downgrade-{id}");
        downgrade.changes.clear();
        downgrade.snapshots = vec![SaveSnapshot {
            object_id: id,
            mutation_revision: 2,
            expected_version: 1,
            bytes,
        }];
        assert!(store.placement_operation(&downgrade).await.is_err());
        assert!(
            store
                .resolve_operation(&downgrade.operation_id)
                .await
                .unwrap()
                .is_none()
        );
    }
    let mut moved = child.clone();
    moved.entity.mutation_revision = 2;
    moved.placement = contained(player.entity.object_id);
    let mut pickup = PlacementOperation {
        operation_id: "v3-pickup".into(),
        snapshots: vec![snapshot(&moved.entity, 1, moved.encode().unwrap())],
        participants: vec![
            corpse.corpse.entity.object_id,
            child.entity.object_id,
            player.entity.object_id,
        ],
        leases: vec![],
        changes: vec![PlacementChange {
            item: child.entity.object_id,
            expected: Some(place(corpse.corpse.entity.object_id)),
            destination: place(player.entity.object_id),
        }],
        storage_views: vec![],
    };
    assert!(store.placement_operation(&pickup).await.is_err());
    assert!(
        store
            .resolve_operation("v3-pickup")
            .await
            .unwrap()
            .is_none()
    );
    pickup.leases.push(lease);
    store.placement_operation(&pickup).await.unwrap();
    assert_eq!(
        store.placement_operation(&pickup).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let loaded = store.begin_login(lease).await.unwrap();
    let tree = store
        .load_character_inventory(loaded.lease, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(tree.items.len(), 1);
    assert_eq!(
        ItemSaveV3::decode(&tree.items[0].aggregate.bytes).unwrap(),
        moved
    );
    assert_eq!(
        CorpseSaveV3::decode(
            &store
                .load(corpse.corpse.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes
        )
        .unwrap(),
        corpse
    );
    let mut upgraded = CorpseSaveV4 {
        previous: corpse.clone(),
        source: Some(0x70000001),
        operation: Some(17),
    };
    upgraded.corpse.entity.mutation_revision = 2;
    let mut upgrade = operation.clone();
    upgrade.operation_id = "v4-corpse-upgrade".into();
    upgrade.changes.clear();
    upgrade.participants = vec![corpse.corpse.entity.object_id];
    upgrade.snapshots = vec![snapshot(
        &upgraded.corpse.entity,
        1,
        upgraded.encode().unwrap(),
    )];
    store.placement_operation(&upgrade).await.unwrap();
    for index in 0..4 {
        let mut changed = upgraded.clone();
        changed.corpse.entity.mutation_revision = 3;
        match index {
            0 => changed.corpse.expires_at += 1,
            1 => changed.source = Some(9),
            2 => changed.operation = Some(18),
            _ => {
                changed.source = None;
                changed.operation = None;
            }
        }
        let mut rejected = upgrade.clone();
        rejected.operation_id = format!("v4-corpse-identity-{index}");
        rejected.snapshots = vec![snapshot(
            &changed.corpse.entity,
            2,
            changed.encode().unwrap(),
        )];
        assert!(store.placement_operation(&rejected).await.is_err());
        assert!(
            store
                .resolve_operation(&rejected.operation_id)
                .await
                .unwrap()
                .is_none()
        );
        let persisted = store
            .load(upgraded.corpse.entity.object_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(CorpseSaveV4::decode(&persisted.bytes).unwrap(), upgraded);
        assert_eq!(persisted.persisted_version, 2);
    }
    store.abort_loading(loaded.lease).await.unwrap();
    store.close().await;
}
#[tokio::test]
async fn house_v3_registry_survives_fenced_routine_save_and_old_writer_rejection() {
    let c = Cluster::start();
    let store = PgStore::connect(&c.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let (player, lease) = player(&store, 74).await;
    let old = HouseSaveV1 {
        entity: entity(0x70000001),
        house_id: 42,
        owner_id: player.entity.object_id,
        purchased_at: 1,
        rent_period_start: 1,
        rent_due_at: 100,
        access: vec![],
    };
    store
        .save_house("v3-house-create", lease, &old, 0)
        .await
        .unwrap();
    let mut house = HouseSaveV3 {
        previous: HouseSaveV2::migrate_v1(old.clone()).unwrap(),
        enchantments: registry(),
    };
    house.entity.mutation_revision = 2;
    let upgrade = HousingOperation {
        inventory: PlacementOperation {
            operation_id: "v3-house-upgrade".into(),
            snapshots: vec![snapshot(&house.entity, 1, house.encode().unwrap())],
            participants: vec![player.entity.object_id, house.entity.object_id],
            leases: vec![lease],
            changes: vec![],
            storage_views: vec![],
        },
        ownership: HouseOwnershipChange {
            house: house.entity.object_id,
            house_id: 42,
            expected_owner: Some(player.entity.object_id),
            expected_generation: 1,
            owner: Some(player.entity.object_id),
            generation: 1,
        },
    };
    store.housing_operation(&upgrade).await.unwrap();
    let mut prior = old;
    prior.entity.mutation_revision = 3;
    assert!(
        store
            .save_house("v3-old-house-writer", lease, &prior, 2)
            .await
            .is_err()
    );
    assert!(
        store
            .resolve_operation("v3-old-house-writer")
            .await
            .unwrap()
            .is_none()
    );
    let mut downgrade = upgrade.clone();
    downgrade.inventory.operation_id = "v3-house-downgrade".into();
    let mut older = house.previous.clone();
    older.entity.mutation_revision = 3;
    downgrade.inventory.snapshots = vec![snapshot(&older.entity, 2, older.encode().unwrap())];
    assert!(store.housing_operation(&downgrade).await.is_err());
    house.entity.mutation_revision = 3;
    house.enchantments[0].start_time = -20.0;
    let mut routine = OwnedSaveBatch {
        snapshots: vec![snapshot(&house.entity, 2, house.encode().unwrap())],
        participants: vec![player.entity.object_id, house.entity.object_id],
        leases: vec![],
    };
    assert!(store.save_owned_batch(&routine).await.is_err());
    routine.leases.push(lease);
    store.save_owned_batch(&routine).await.unwrap();
    assert_eq!(
        HouseSaveV3::decode(
            &store
                .load(house.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes
        )
        .unwrap(),
        house
    );
    let mut forged = house.clone();
    forged.owner_id = None;
    forged.entity.mutation_revision = 4;
    routine.snapshots = vec![snapshot(&forged.entity, 3, forged.encode().unwrap())];
    assert!(store.save_owned_batch(&routine).await.is_err());
    assert_eq!(
        HouseSaveV3::decode(
            &store
                .load(house.entity.object_id)
                .await
                .unwrap()
                .unwrap()
                .bytes
        )
        .unwrap(),
        house
    );
    store.close().await;
}
