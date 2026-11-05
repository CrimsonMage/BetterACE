use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_content::{Attribute, Property, SecondaryAttribute, Skill, WeenieV1};
use bace_db_postgres::PgStore;
use bace_persistence::SaveSnapshot;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};

#[tokio::test]
async fn creation_allocations_survive_exact_postgres_login_snapshot() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let account = store
        .create(NewAccount {
            name: AccountName::parse("creation-allocations").unwrap(),
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
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "allocation_player".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    let allocated = [100, 75, 55, 35, 20, 10];
    for (attribute_id, init_level) in [1, 2, 4, 3, 5, 6].into_iter().zip(allocated) {
        state.properties.attributes.push(Property {
            id: attribute_id,
            value: Attribute {
                init_level,
                level_from_cp: 0,
                cp_spent: 0,
            },
        });
    }
    state.properties.secondary_attributes = vec![
        Property {
            id: 1,
            value: SecondaryAttribute {
                init_level: 150,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 150,
            },
        },
        Property {
            id: 3,
            value: SecondaryAttribute {
                init_level: 120,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 120,
            },
        },
        Property {
            id: 5,
            value: SecondaryAttribute {
                init_level: 110,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 110,
            },
        },
    ];
    state.properties.skills = vec![
        Property {
            id: 13,
            value: Skill {
                sac: 2,
                init_level: 27,
                ..Default::default()
            },
        },
        Property {
            id: 14,
            value: Skill {
                sac: 3,
                init_level: 54,
                ..Default::default()
            },
        },
    ];
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state,
        },
        account_id: account.id.0,
        name: "Creation Allocations".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let offline = store.create_player(&player, 0, 11, &[]).await.unwrap();
    store.close().await;

    let reopened = PgStore::connect(&cluster.url(), 2).await.unwrap();
    let loading = reopened.begin_login(offline).await.unwrap();
    let loaded = PlayerSaveV1::decode(&loading.snapshot.bytes).unwrap();
    assert_eq!(loaded, player);
    assert_eq!(
        loaded
            .entity
            .state
            .properties
            .attributes
            .iter()
            .map(|attribute| attribute.value.init_level)
            .collect::<Vec<_>>(),
        allocated
    );
    assert_eq!(
        loaded
            .entity
            .state
            .properties
            .skills
            .iter()
            .map(|skill| (skill.id, skill.value.sac, skill.value.init_level))
            .collect::<Vec<_>>(),
        vec![(13, 2, 27), (14, 3, 54)]
    );
    let offline = reopened.abort_loading(loading.lease).await.unwrap();
    let mut migrated = PlayerSaveV6::migrate_v1(loaded).unwrap();
    migrated.player.entity.mutation_revision = 2;
    let bytes = migrated.encode().unwrap();
    reopened
        .save_offline(
            offline,
            &SaveSnapshot {
                object_id: id,
                mutation_revision: 2,
                expected_version: 1,
                bytes,
            },
        )
        .await
        .unwrap();
    reopened.close().await;

    let reopened = PgStore::connect(&cluster.url(), 2).await.unwrap();
    let loading = reopened.begin_login(offline).await.unwrap();
    assert_eq!(
        PlayerSaveV6::decode(&loading.snapshot.bytes).unwrap(),
        migrated
    );
    reopened.close().await;
}
