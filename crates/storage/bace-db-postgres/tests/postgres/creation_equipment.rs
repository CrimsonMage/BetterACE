use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_content::{Property, WeenieV1};
use bace_db_postgres::PgStore;
use bace_persistence::InventoryLoadLimits;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};

#[tokio::test]
async fn creation_equipment_is_placed_and_old_creation_is_repaired_once() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let account = store
        .create(NewAccount {
            name: AccountName::parse("creation-gear").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"local-test-password")
                .unwrap(),
        })
        .await
        .unwrap();
    let CreateAccountOutcome::Created(account) = account else {
        panic!("fresh account")
    };
    let id = store.allocate_player_id().await.unwrap();
    let template = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "creation-gear".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: template.clone(),
        },
        account_id: account.id.0,
        name: "Creation Gear".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let mut clothing = template.clone();
    clothing
        .properties
        .instance_ids
        .push(Property { id: 3, value: id });
    clothing.properties.ints.push(Property { id: 10, value: 8 });
    let mut inventory = template.clone();
    inventory
        .properties
        .instance_ids
        .push(Property { id: 2, value: id });
    // Template ValidLocations does not make a carried item equipped.
    inventory
        .properties
        .ints
        .push(Property { id: 10, value: 32 });
    let clothing_id = 0x8000_0001;
    let inventory_id = 0x8000_0002;
    let bag_id = 0x8000_0003;
    let mut bag = template;
    bag.weenie_type = 21;
    bag.properties
        .instance_ids
        .push(Property { id: 2, value: id });
    let lease = store
        .create_player(
            &player,
            0,
            11,
            &[
                (
                    EntitySaveV1 {
                        object_id: clothing_id,
                        template_revision: 1,
                        mutation_revision: 1,
                        state: clothing,
                    },
                    0,
                ),
                (
                    EntitySaveV1 {
                        object_id: inventory_id,
                        template_revision: 1,
                        mutation_revision: 1,
                        state: inventory,
                    },
                    1,
                ),
                (
                    EntitySaveV1 {
                        object_id: bag_id,
                        template_revision: 1,
                        mutation_revision: 1,
                        state: bag,
                    },
                    2,
                ),
            ],
        )
        .await
        .unwrap();
    let loading = store.begin_login(lease).await.unwrap();
    let loaded = store
        .load_character_inventory(loading.lease, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(
        loaded
            .items
            .iter()
            .find(|item| item.aggregate.object_id == clothing_id)
            .unwrap()
            .equipped,
        8
    );
    assert_eq!(
        loaded
            .items
            .iter()
            .find(|item| item.aggregate.object_id == inventory_id)
            .unwrap()
            .equipped,
        0
    );
    let bag = loaded
        .items
        .iter()
        .find(|item| item.aggregate.object_id == bag_id)
        .unwrap();
    assert!(bag.pack_slot);
    assert_eq!(bag.location.slot, 0);

    // Reproduce the pre-fix relational row while retaining its frozen creation snapshot.
    sqlx::query("UPDATE item_ownership SET equipped=0 WHERE item_id=$1")
        .bind(i64::from(clothing_id))
        .execute(&sqlx::PgPool::connect(&cluster.url()).await.unwrap())
        .await
        .unwrap();
    sqlx::query("UPDATE item_ownership SET pack_slot=false,slot=2 WHERE item_id=$1")
        .bind(i64::from(bag_id))
        .execute(&sqlx::PgPool::connect(&cluster.url()).await.unwrap())
        .await
        .unwrap();
    let repaired = store
        .load_character_inventory(loading.lease, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(
        repaired
            .items
            .iter()
            .find(|item| item.aggregate.object_id == clothing_id)
            .unwrap()
            .equipped,
        8
    );
    let bag = repaired
        .items
        .iter()
        .find(|item| item.aggregate.object_id == bag_id)
        .unwrap();
    assert!(bag.pack_slot);
    assert_eq!(bag.location.slot, 0);
    assert!(
        store
            .resolve_operation(&format!("creation-equipment-repair:{id}"))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .resolve_operation(&format!("creation-pack-repair:{id}"))
            .await
            .unwrap()
            .is_some()
    );
    let again = store
        .load_character_inventory(loading.lease, InventoryLoadLimits::default())
        .await
        .unwrap();
    assert_eq!(
        again
            .items
            .iter()
            .find(|item| item.aggregate.object_id == clothing_id)
            .unwrap()
            .equipped,
        8
    );
    store.close().await;
}
