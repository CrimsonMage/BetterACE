use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::{PgStore, StoreError};
use bace_persistence::{AllegianceCommit, AllegianceOperation, AllegianceWrite, SaveSnapshot};
use bace_storage_codec::{AllegianceNodeV1, EntitySaveV1, PlayerSaveV1};

#[tokio::test]
async fn allegiance_edges_xp_and_receipts_commit_together_and_reject_stale_or_cyclic_patches() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("allegiance-test").unwrap(),
            password_hash: PasswordService::new(1).unwrap().hash(b"test-only").unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("fresh account")
    };
    let mut players = Vec::new();
    let mut leases = Vec::new();
    let mut nodes = Vec::new();
    for i in 0..3u32 {
        let id = 0x50000001 + i;
        let player = PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: id,
                template_revision: 1,
                mutation_revision: 1,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 1,
                    class_name: "allegiance_player".into(),
                    weenie_type: 10,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            account_id: account.id.0,
            name: format!("Allegiance {i}"),
            metadata: Default::default(),
            quests: vec![],
        };
        leases.push(
            store
                .create_player(&player, i as u16, 11, &[])
                .await
                .unwrap(),
        );
        nodes.push(AllegianceNodeV1 {
            character: id,
            account: account.id.0,
            name: player.name.clone(),
            gender: 1,
            heritage: 1,
            followers: 0,
            patron: None,
            monarch: id,
            vassals: vec![],
            rank: 1,
            level: 100,
            leadership: 291,
            loyalty: 291,
            sworn_at: 0,
            online_seconds: 1,
            may_pass_up: true,
            received_total: 0,
            tithed_total: 0,
            unclaimed: 0,
        });
        players.push(player);
    }
    let write = |node: &AllegianceNodeV1, expected_version, mutation_revision| AllegianceWrite {
        character: node.character,
        expected_version,
        mutation_revision,
        bytes: Some(node.encode().unwrap()),
    };
    let initial = AllegianceOperation {
        operation_id: "allegiance-init".into(),
        nodes: nodes.iter().map(|n| write(n, 0, 1)).collect(),
        metadata: vec![],
        players: vec![],
        leases: leases.clone(),
    };
    assert!(matches!(
        store.allegiance_operation(&initial).await.unwrap(),
        AllegianceCommit::Committed { .. }
    ));
    assert_eq!(
        store.allegiance_operation(&initial).await.unwrap(),
        AllegianceCommit::AlreadyCommitted
    );
    nodes[0].vassals = vec![nodes[1].character];
    nodes[1].patron = Some(nodes[0].character);
    nodes[1].monarch = nodes[0].character;
    let mut sworn = AllegianceOperation {
        operation_id: "swear".into(),
        nodes: vec![write(&nodes[0], 1, 2), write(&nodes[1], 1, 2)],
        metadata: vec![],
        players: vec![],
        leases: leases.clone(),
    };
    store.allegiance_operation(&sworn).await.unwrap();
    sworn.nodes[0].mutation_revision += 1;
    assert!(matches!(
        store.allegiance_operation(&sworn).await,
        Err(StoreError::OperationMismatch)
    ));
    let before = store
        .load_allegiance_nodes(&[nodes[0].character, nodes[1].character])
        .await
        .unwrap();
    let mut cyclic = nodes.clone();
    cyclic[0].patron = Some(cyclic[1].character);
    cyclic[1].vassals = vec![cyclic[0].character];
    let cycle = AllegianceOperation {
        operation_id: "cycle-reject".into(),
        nodes: vec![write(&cyclic[0], 2, 3), write(&cyclic[1], 2, 3)],
        metadata: vec![],
        players: vec![],
        leases: leases.clone(),
    };
    assert!(store.allegiance_operation(&cycle).await.is_err());
    assert!(
        store
            .resolve_operation("cycle-reject")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .load_allegiance_nodes(&[nodes[0].character, nodes[1].character])
            .await
            .unwrap(),
        before
    );
    nodes[0].received_total = 123;
    nodes[1].tithed_total = 123;
    players[0].entity.mutation_revision = 2;
    players[0]
        .entity
        .state
        .properties
        .int64s
        .push(bace_content::Property { id: 1, value: 123 });
    let mut xp = AllegianceOperation {
        operation_id: "xp-and-lineage".into(),
        nodes: vec![write(&nodes[0], 2, 3), write(&nodes[1], 2, 3)],
        metadata: vec![],
        players: vec![SaveSnapshot {
            object_id: nodes[0].character,
            expected_version: 99,
            mutation_revision: 2,
            bytes: players[0].encode().unwrap(),
        }],
        leases,
    };
    assert!(matches!(
        store.allegiance_operation(&xp).await,
        Err(StoreError::Conflict(_))
    ));
    assert_eq!(
        store
            .load_allegiance_nodes(&[nodes[0].character, nodes[1].character])
            .await
            .unwrap(),
        before
    );
    xp.players[0].expected_version = 1;
    assert!(matches!(
        store.allegiance_operation(&xp).await.unwrap(),
        AllegianceCommit::Committed { .. }
    ));
    assert_eq!(
        store.allegiance_operation(&xp).await.unwrap(),
        AllegianceCommit::AlreadyCommitted
    );
    assert_eq!(
        store.load(nodes[0].character).await.unwrap().unwrap().bytes,
        xp.players[0].bytes
    );
    let forest = store.load_allegiance_forest(3, 1024 * 1024).await.unwrap();
    assert_eq!(forest.0.len(), 3);
    assert!(forest.1.is_empty());
    assert!(store.load_allegiance_forest(2, 1024 * 1024).await.is_err());
    assert!(store.load_allegiance_forest(3, 1).await.is_err());
    let owner = store.acquire_world_owner().await.unwrap();
    let corpse_id = 0x80000001;
    let position = bace_content::Position {
        obj_cell_id: 0x12340001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    };
    let mut corpse_entity = players[0].entity.clone();
    corpse_entity.object_id = corpse_id;
    corpse_entity.mutation_revision = 1;
    let corpse = bace_storage_codec::CorpseSaveV3::migrate_v2(bace_storage_codec::CorpseSaveV2 {
        corpse: bace_storage_codec::CorpseSaveV1 {
            entity: corpse_entity,
            owner: Some(nodes[0].character),
            death_operation: "joint-death".into(),
            expires_at: 1000,
        },
        placement: bace_storage_codec::ItemPlacementV2::World(position.clone()),
    })
    .unwrap();
    players[0].entity.mutation_revision = 3;
    players[0].entity.state.properties.int64s[0].value = 456;
    nodes[0].received_total = 456;
    let ledger = AllegianceOperation {
        operation_id: "joint-death".into(),
        nodes: vec![write(&nodes[0], 3, 4)],
        metadata: vec![],
        players: vec![],
        leases: xp.leases.clone(),
    };
    let placement = bace_persistence::PlacementOperation {
        operation_id: "joint-death".into(),
        snapshots: vec![
            SaveSnapshot {
                object_id: corpse_id,
                mutation_revision: 1,
                expected_version: 0,
                bytes: corpse.encode().unwrap(),
            },
            SaveSnapshot {
                object_id: nodes[0].character,
                mutation_revision: 3,
                expected_version: 99,
                bytes: players[0].encode().unwrap(),
            },
        ],
        participants: nodes
            .iter()
            .map(|n| n.character)
            .chain([corpse_id])
            .collect(),
        leases: xp.leases.clone(),
        changes: vec![bace_persistence::PlacementChange {
            item: corpse_id,
            expected: None,
            destination: bace_persistence::DurableItemPlace::World {
                cell: position.obj_cell_id,
            },
        }],
        storage_views: vec![],
    };
    let mut joint = bace_persistence::AllegiancePlacementOperation {
        placement,
        allegiance: ledger,
        world_epoch: owner.epoch(),
        workflow: None,
    };
    assert!(store.allegiance_placement_operation(&joint).await.is_err());
    assert!(store.load(corpse_id).await.unwrap().is_none());
    assert!(store.item_place(corpse_id).await.unwrap().is_none());
    assert!(
        store
            .resolve_operation("joint-death")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.load_allegiance_forest(3, 1024 * 1024).await.unwrap(),
        forest
    );
    joint.placement.snapshots[1].expected_version = 2;
    assert!(matches!(
        store.allegiance_placement_operation(&joint).await.unwrap(),
        bace_persistence::OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.allegiance_placement_operation(&joint).await.unwrap(),
        bace_persistence::OperationOutcome::AlreadyCommitted
    );
    assert_eq!(
        store.item_place(corpse_id).await.unwrap(),
        Some(bace_persistence::DurableItemPlace::World {
            cell: position.obj_cell_id
        })
    );
    joint.allegiance.nodes[0].mutation_revision += 1;
    assert!(matches!(
        store.allegiance_placement_operation(&joint).await,
        Err(StoreError::OperationMismatch)
    ));
    owner.close().await.unwrap();
    store.close().await;
}
