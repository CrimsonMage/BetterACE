use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_content::{Position, Property, WeenieV1};
use bace_db_postgres::PgStore;
use bace_gameplay_api::{RareAward, RareDecision};
use bace_persistence::{
    OperationOutcome, PlacementOperation, SaveSnapshot, WorldPlacementOperation,
};
use bace_runtime::death_saves::{DeathFreezeInput, DeathPlayer, freeze_native_death};
use bace_storage_codec::{EntitySaveV1, ItemSaveV5, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};
use bace_types::EntityId;
fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "death_fixture".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}

#[tokio::test]
async fn zero_drop_player_no_corpse_checkpoint_has_exact_replay_receipt() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("nocorpseplayer").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("account creation")
    };
    let id = store.allocate_player_id().await.unwrap();
    let player = PlayerSaveV1 {
        entity: entity(id),
        account_id: account.id.0,
        name: "NoCorpse Player".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let offline = store.create_player(&player, 0, 11, &[]).await.unwrap();
    let online = store
        .finish_login(store.begin_login(offline).await.unwrap().lease)
        .await
        .unwrap();
    let mut checkpoint =
        PlayerSaveV6::migrate_v2(PlayerSaveV2::migrate_v1(player).unwrap()).unwrap();
    checkpoint.player.entity.mutation_revision = 2;
    checkpoint.ui.spellbook_filters = 0x77;
    let operation = WorldPlacementOperation {
        world_epoch: owner.epoch(),
        inventory: PlacementOperation {
            operation_id: format!("player-death:{}:8", owner.epoch()),
            snapshots: vec![SaveSnapshot {
                object_id: id,
                mutation_revision: 2,
                expected_version: 1,
                bytes: checkpoint.encode().unwrap(),
            }],
            participants: vec![id],
            leases: vec![online],
            changes: vec![],
            storage_views: vec![],
        },
    };
    assert!(matches!(
        store.world_placement_operation(&operation).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.world_placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let saved = store.load(id).await.unwrap().unwrap();
    assert_eq!(saved.persisted_version, 2);
    let saved = PlayerSaveV6::decode(&saved.bytes).unwrap();
    assert_eq!(saved.player.entity.mutation_revision, 2);
    assert_eq!(saved.ui.spellbook_filters, 0x77);
    let mut changed = operation.clone();
    changed.inventory.snapshots[0].mutation_revision = 3;
    assert!(store.world_placement_operation(&changed).await.is_err());
    store.close().await;
}
#[tokio::test]
async fn rare_advance_corpse_loot_and_xp_commit_once_or_all_roll_back() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    store.bind_random_key(1, [44; 32]).await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("deathplayer").unwrap(),
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
    let id = store.allocate_player_id().await.unwrap();
    let mut player = PlayerSaveV1 {
        entity: entity(id),
        account_id: account.id.0,
        name: "Death Player".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    player
        .entity
        .state
        .properties
        .int64s
        .push(Property { id: 2, value: 10 });
    let lease = store.create_player(&player, 0, 11, &[]).await.unwrap();
    let saved = PlayerSaveV6::migrate_v2(PlayerSaveV2::migrate_v1(player).unwrap()).unwrap();
    let root = bace_random::RandomRoot::new([44; 32], 1).unwrap();
    let previous = bace_runtime::rare_saves::initial_rare_state(&root, id).unwrap();
    let mut next = previous;
    next.attempt_ordinal = 1;
    next.timer_ordinal = 1;
    next.last_effective_time = 100;
    next.next_realtime_at = Some(200);
    let decision = RareDecision {
        character: id,
        eligible: true,
        previous,
        next,
        profile_id: 1,
        standard_success: true,
        realtime_success: false,
        award: Some(RareAward {
            tier: 1,
            template: 1,
        }),
    };
    let drop = bace_loot::LootDrop {
        template: 1,
        stack: 1,
        node: "ordinary".into(),
        mutations: vec![bace_gameplay_api::GeneratedItemMutation::Int(19, 1234)],
    };
    let proposal = bace_simulation::DeathProposal {
        social: None,
        position: Some(Position {
            obj_cell_id: 0x12340001,
            position_x: 1.0,
            position_y: 2.0,
            position_z: 3.0,
            rotation_w: 1.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
        }),
        no_corpse: false,
        olthoi_killer: false,
        operation: 1,
        victim: EntityId(9),
        owner: Some(EntityId(id)),
        corpse_template: 1,
        corpse_decay_ticks: 300,
        drops: vec![
            bace_simulation::LootDrop {
                template: 1,
                stack: 1
            };
            2
        ],
        experience: vec![(EntityId(id), 50)],
        experience_state: vec![(
            EntityId(id),
            bace_character::ExperienceCredit {
                before_revision: 1,
                after_revision: 2,
                before_available: 10,
                after_available: 60,
            },
        )],
        native: Some(bace_simulation::NativeDeathLoot {
            source_parents: None,
            source_items: None,
            event_id: [1; 16],
            key_version: 1,
            graph_id: 1,
            graph_revision: [2; 32],
            content_generation: [3; 32],
            rare_profile_revision: Some([4; 32]),
            generated: vec![
                drop,
                bace_loot::LootDrop {
                    template: 1,
                    stack: 1,
                    node: "$rare".into(),
                    mutations: vec![],
                },
            ],
            rare: Some(decision),
        }),
    };
    let corpse = entity(0x80000001);
    let items = [entity(0x80000002), entity(0x80000003)];
    let players = [DeathPlayer {
        saved: saved.clone(),
        persisted_version: 1,
    }];
    let freeze = |proposal: &bace_simulation::DeathProposal| {
        freeze_native_death(DeathFreezeInput {
            proposal,
            corpse: corpse.clone(),
            position: Position {
                obj_cell_id: 0x12340001,
                position_x: 1.0,
                position_y: 2.0,
                position_z: 3.0,
                rotation_w: 1.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
            },
            expires_at: 1000,
            items: &items[..proposal.drops.len()],
            players: &players,
            leases: &[lease],
        })
    };
    let mut missing = proposal.clone();
    missing.drops.pop();
    missing.native.as_mut().unwrap().generated.pop();
    assert!(freeze(&missing).is_err());
    let mut duplicate = proposal.clone();
    let generated = &mut duplicate.native.as_mut().unwrap().generated;
    generated[0] = generated[1].clone();
    assert!(freeze(&duplicate).is_err());
    let mut mismatch = proposal.clone();
    mismatch
        .native
        .as_mut()
        .unwrap()
        .rare
        .as_mut()
        .unwrap()
        .award
        .as_mut()
        .unwrap()
        .template = 999;
    assert!(freeze(&mismatch).is_err());
    let mut mutated = proposal.clone();
    mutated.native.as_mut().unwrap().generated[1]
        .mutations
        .push(bace_gameplay_api::GeneratedItemMutation::Int(19, 1234));
    assert!(freeze(&mutated).is_err());
    let mut unexpected = proposal.clone();
    let rare = unexpected.native.as_mut().unwrap().rare.as_mut().unwrap();
    rare.award = None;
    rare.standard_success = false;
    assert!(freeze(&unexpected).is_err());
    let mut orphan = proposal.clone();
    orphan.experience_state.clear();
    orphan.experience.clear();
    orphan
        .native
        .as_mut()
        .unwrap()
        .rare
        .as_mut()
        .unwrap()
        .eligible = false;
    assert!(
        freeze_native_death(DeathFreezeInput {
            proposal: &orphan,
            corpse: corpse.clone(),
            position: Position {
                obj_cell_id: 0x12340001,
                position_x: 1.0,
                position_y: 2.0,
                position_z: 3.0,
                rotation_w: 1.0,
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0
            },
            expires_at: 1000,
            items: &items,
            players: &[],
            leases: &[],
        })
        .is_err()
    );
    let operation = freeze(&proposal).unwrap();
    let mut stale = operation.clone();
    stale
        .snapshots
        .iter_mut()
        .find(|s| s.object_id == id)
        .unwrap()
        .expected_version = 9;
    assert!(store.placement_operation(&stale).await.is_err());
    assert!(store.load(0x80000001).await.unwrap().is_none());
    assert!(store.load(0x80000002).await.unwrap().is_none());
    assert!(matches!(
        store.placement_operation(&operation).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.placement_operation(&operation).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let durable = PlayerSaveV6::decode(&store.load(id).await.unwrap().unwrap().bytes).unwrap();
    assert_eq!(durable.rares.unwrap().attempt_ordinal, 1);
    assert_eq!(durable.player.entity.mutation_revision, 2);
    assert_eq!(durable.ui, saved.ui);
    assert_eq!(
        durable
            .player
            .entity
            .state
            .properties
            .int64s
            .iter()
            .find(|p| p.id == 2)
            .unwrap()
            .value,
        60
    );
    let loot = ItemSaveV5::decode(&store.load(0x80000002).await.unwrap().unwrap().bytes).unwrap();
    assert_eq!(
        loot.entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 19)
            .unwrap()
            .value,
        1234
    );
    assert!(store.placement_operation(&stale).await.is_err());
    assert_eq!(store.load(id).await.unwrap().unwrap().persisted_version, 2);
    store.close().await;
}
