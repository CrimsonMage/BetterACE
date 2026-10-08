use super::Cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::PgStore;
use bace_persistence::*;
use bace_storage_codec::{npc_values_v1::*, npc_workflow_v1::*, *};
const NPC: u32 = 0x80000001;
#[tokio::test]
async fn source_inventory_commits_with_head_and_later_tombstone_is_never_recreated() {
    use bace_storage_codec::npc_workflow_v3::{
        NpcSourceInventoryItemV3, NpcSourceInventoryV3, NpcSourceLocationV3,
    };
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    store.bind_random_key(1, [44; 32]).await.unwrap();
    let id = 0x80000002;
    let placement = ItemPlacementV2::Contained {
        container: NPC,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    let entity = EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "npc_saved_gear".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    };
    let mut item = ItemSaveV4 {
        previous: ItemSaveV3 {
            previous: ItemSaveV2 { entity, placement },
            enchantments: vec![],
        },
        construction: None,
    };
    let mut frozen = NpcWorkflowSaveV3::from(checkpoint(0x50000001));
    frozen.source_version = 1;
    frozen.source_template = 100;
    frozen.location = Some(NpcSourceLocationV3 {
        cell: 0x12340001,
        position: [1., 2., 3.],
        heading: 0.,
        player: false,
        creature: true,
    });
    frozen.inventory = Some(NpcSourceInventoryV3 {
        source_persisted_version: 1,
        source_mutation_revision: 1,
        origin: None,
        ticket: 91,
        source_registry_revision: None,
        source_enchantments: vec![],
        death_items: vec![id],
        items: vec![NpcSourceInventoryItemV3 {
            id,
            revision: 1,
            persisted_version: 1,
            registry_revision: None,
            container: NPC,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }],
    });
    let place = DurableItemPlace::Contained {
        container: NPC,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    let operation = NpcStageOperation {
        inventory: PlacementOperation {
            operation_id: "npc-gear-initial".into(),
            snapshots: vec![
                SaveSnapshot {
                    object_id: NPC,
                    mutation_revision: 1,
                    expected_version: 0,
                    bytes: EntitySaveV1 {
                        object_id: NPC,
                        template_revision: 1,
                        mutation_revision: 1,
                        state: bace_content::WeenieV1 {
                            schema_version: 1,
                            weenie_id: 100,
                            class_name: "npc_root".into(),
                            weenie_type: 10,
                            last_modified: None,
                            properties: Default::default(),
                        },
                    }
                    .encode_item()
                    .unwrap(),
                },
                SaveSnapshot {
                    object_id: id,
                    mutation_revision: 1,
                    expected_version: 0,
                    bytes: item.encode().unwrap(),
                },
            ],
            participants: vec![NPC, id],
            leases: vec![],
            changes: vec![PlacementChange {
                item: id,
                expected: None,
                destination: place,
            }],
            storage_views: vec![],
        },
        workflow: NpcWorkflowUpdate {
            invocation: frozen.invocation,
            world_epoch: owner.epoch(),
            expected_version: 0,
            checkpoint: frozen.encode().unwrap(),
        },
    };
    store.npc_stage(&operation).await.unwrap();
    assert_eq!(
        store.load_npc_source_inventory(NPC, 1).await.unwrap().len(),
        1
    );
    assert_eq!(
        store
            .npc_sources_outside_region(&[NPC], 0x1235)
            .await
            .unwrap(),
        vec![NPC]
    );
    assert!(
        store
            .npc_sources_outside_region(&[NPC], 0x1234)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .npc_source_heads_in_region(0x1234, None, 16)
            .await
            .unwrap()[0]
            .source,
        NPC
    );
    item.previous.previous.entity.mutation_revision = 2;
    item.previous.enchantments = vec![FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 7,
        spell_id: 123,
        layer_id: 3,
        has_spell_set_id: false,
        spell_category: 23,
        power_level: 100,
        start_time: -15.,
        duration: 60.,
        caster_object_id: 7,
        degrade_modifier: 0.1,
        degrade_limit: 0.5,
        last_time_degraded: -2.,
        stat_mod_type: 0x1000,
        stat_mod_key: 7,
        stat_mod_value: -10.,
        spell_set_id: 42,
    }];
    store
        .world_placement_operation(&WorldPlacementOperation {
            world_epoch: owner.epoch(),
            inventory: PlacementOperation {
                operation_id: "npc-gear-later-registry".into(),
                snapshots: vec![SaveSnapshot {
                    object_id: id,
                    mutation_revision: 2,
                    expected_version: 1,
                    bytes: item.encode().unwrap(),
                }],
                participants: vec![NPC, id],
                leases: vec![],
                changes: vec![PlacementChange {
                    item: id,
                    expected: Some(place),
                    destination: place,
                }],
                storage_views: vec![],
            },
        })
        .await
        .unwrap();
    let current = store.load_npc_source_inventory(NPC, 1).await.unwrap();
    assert_eq!(current[0].aggregate.persisted_version, 2);
    assert_eq!(
        ItemSaveV5::decode_or_migrate(&current[0].aggregate.bytes, None)
            .unwrap()
            .enchantments,
        item.enchantments
    );
    item.previous.previous.entity.mutation_revision = 3;
    item.previous.previous.placement = ItemPlacementV2::Removed;
    store
        .world_placement_operation(&WorldPlacementOperation {
            world_epoch: owner.epoch(),
            inventory: PlacementOperation {
                operation_id: "npc-gear-later-remove".into(),
                snapshots: vec![SaveSnapshot {
                    object_id: id,
                    mutation_revision: 3,
                    expected_version: 2,
                    bytes: item.encode().unwrap(),
                }],
                participants: vec![NPC, id],
                leases: vec![],
                changes: vec![PlacementChange {
                    item: id,
                    expected: Some(place),
                    destination: DurableItemPlace::Removed,
                }],
                storage_views: vec![],
            },
        })
        .await
        .unwrap();
    assert!(
        store
            .load_npc_source_inventory(NPC, 1)
            .await
            .unwrap()
            .is_empty()
    );
    frozen.previous.previous.stage = 1;
    frozen.source_version = 2;
    let stale = NpcStageOperation {
        inventory: PlacementOperation {
            operation_id: "npc-gear-stale-proof".into(),
            snapshots: vec![],
            participants: vec![NPC, id],
            leases: vec![],
            changes: vec![],
            storage_views: vec![],
        },
        workflow: NpcWorkflowUpdate {
            invocation: frozen.invocation,
            world_epoch: owner.epoch(),
            expected_version: 1,
            checkpoint: frozen.encode().unwrap(),
        },
    };
    assert!(store.npc_stage(&stale).await.is_err());
    assert_eq!(
        store
            .npc_source_head(NPC)
            .await
            .unwrap()
            .unwrap()
            .source_version,
        1
    );
    owner.close().await.unwrap();
    store.close().await;
}
fn checkpoint(target: u32) -> NpcWorkflowSaveV1 {
    NpcWorkflowSaveV1 {
        invocation: [7; 16],
        source: NPC,
        program_hash: [8; 32],
        content_generation: [9; 32],
        logical_now: 0.0,
        event_id: [10; 16],
        key_version: 1,
        random_position: 2,
        stage: 0,
        completed: false,
        next_order: 1,
        remaining_instructions: 100,
        active_operation: 1,
        invocations: vec![NpcInvocationSaveV1 {
            operation: 1,
            event_id: [10; 16],
            key_version: 1,
            random_position: 2,
        }],
        scheduled: vec![NpcScheduledRowV1 {
            inline: false,
            depth: 0,
            set: 0,
            action: 0,
            due: 2.0,
            order: 1,
            context: NpcContextV1 {
                source: NPC,
                target: Some(target),
                operation: 1,
            },
        }],
        pending: vec![],
        detached: vec![],
        effects: vec![],
    }
}
fn stage(id: &str, version: i64, checkpoint: &NpcWorkflowSaveV1) -> NpcStageOperation {
    NpcStageOperation {
        inventory: PlacementOperation {
            operation_id: id.into(),
            snapshots: vec![],
            participants: vec![NPC],
            leases: vec![],
            changes: vec![],
            storage_views: vec![],
        },
        workflow: NpcWorkflowUpdate {
            invocation: checkpoint.invocation,
            expected_version: version,
            world_epoch: 1,
            checkpoint: checkpoint.encode().unwrap(),
        },
    }
}
#[tokio::test]
async fn npc_stages_commit_effect_and_continuation_together_without_rolling_back_earlier_stages() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    assert_eq!(owner.epoch(), 1);
    store.bind_random_key(1, [44; 32]).await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("npcstages").unwrap(),
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
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "workflow_fixture".into(),
                weenie_type: 24,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Workflow Player".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&player, 0, 1, &[]).await.unwrap();
    let initial = checkpoint(id);
    let begin = stage("npc-begin", 0, &initial);
    assert!(matches!(
        store.npc_stage(&begin).await.unwrap(),
        OperationOutcome::Committed(_)
    ));
    assert_eq!(
        store.npc_stage(&begin).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut next = initial.clone();
    next.stage = 1;
    next.logical_now = 2.0;
    next.scheduled[0].action = 1;
    next.scheduled[0].due = 5.0;
    next.remaining_instructions = 99;
    let mut value = PlayerSaveV4::migrate_v1(player).unwrap();
    value.player.entity.mutation_revision = 2;
    value
        .player
        .entity
        .state
        .properties
        .int64s
        .push(bace_content::Property { id: 2, value: 60 });
    let mut award = stage("npc-award", 1, &next);
    award.inventory.participants.push(id);
    award.inventory.leases.push(lease);
    award.inventory.snapshots.push(SaveSnapshot {
        object_id: id,
        mutation_revision: 2,
        expected_version: 9,
        bytes: value.encode().unwrap(),
    });
    assert!(store.npc_stage(&award).await.is_err());
    assert!(
        store
            .resolve_operation("npc-award")
            .await
            .unwrap()
            .is_none()
    );
    let recovered = store.pending_npc_workflows(NPC, None, 1).await.unwrap();
    assert_eq!(recovered[0].version, 1);
    assert_eq!(
        NpcWorkflowSaveV1::decode(&recovered[0].checkpoint).unwrap(),
        initial
    );
    award.inventory.snapshots[0].expected_version = 1;
    store.npc_stage(&award).await.unwrap();
    assert_eq!(
        store.npc_stage(&award).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut later = next.clone();
    later.stage = 2;
    later.logical_now = 5.0;
    later.scheduled.clear();
    later.completed = true;
    later.remaining_instructions = 98;
    let mut bad_later = stage("npc-later", 2, &later);
    bad_later.inventory.participants.push(id);
    bad_later.inventory.leases.push(lease);
    bad_later.inventory.snapshots = award.inventory.snapshots.clone();
    assert!(store.npc_stage(&bad_later).await.is_err());
    let durable = PlayerSaveV4::decode(&store.load(id).await.unwrap().unwrap().bytes).unwrap();
    assert_eq!(durable.player.entity.state.properties.int64s[0].value, 60);
    let recovered = store.pending_npc_workflows(NPC, None, 1).await.unwrap();
    assert_eq!(recovered[0].version, 2);
    assert_eq!(
        NpcWorkflowSaveV1::decode(&recovered[0].checkpoint).unwrap(),
        next
    );
    // Upgrade an existing V1 workflow to V2 without changing its invocation,
    // source definition, world ownership or exact operation replay identity.
    let mut finish = stage("npc-finish", 2, &later);
    finish.workflow.checkpoint = NpcWorkflowSaveV2::from(later.clone()).encode().unwrap();
    store.npc_stage(&finish).await.unwrap();
    assert_eq!(
        store.npc_stage(&finish).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    assert!(
        store
            .pending_npc_workflows(NPC, None, 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .npc_stage(&stage("npc-rewind", 3, &initial))
            .await
            .is_err()
    );
    owner.close().await.unwrap();
    let newer = store.acquire_world_owner().await.unwrap();
    assert_eq!(newer.epoch(), 2);
    let mut obsolete = initial.clone();
    obsolete.invocation = [11; 16];
    assert!(
        store
            .npc_stage(&stage("npc-stale-world", 0, &obsolete))
            .await
            .is_err()
    );
    newer.close().await.unwrap();
    store.close().await;
}

fn stage_v3(
    id: &str,
    version: i64,
    source_version: u64,
    checkpoint: &NpcWorkflowSaveV1,
) -> NpcStageOperation {
    let mut operation = stage(id, version, checkpoint);
    let mut frozen = NpcWorkflowSaveV3::from(checkpoint.clone());
    frozen.source_version = source_version;
    frozen.source_template = 100;
    operation.workflow.checkpoint = frozen.encode().unwrap();
    operation
}
#[tokio::test]
async fn source_head_cas_orders_overlapping_invocations_and_retains_completed_version() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    store.bind_random_key(1, [44; 32]).await.unwrap();
    let initial = checkpoint(0x50000001);
    let first = stage_v3("head-first", 0, 1, &initial);
    store.npc_stage(&first).await.unwrap();
    assert_eq!(
        store.pending_npc_source_ids(None, 16).await.unwrap(),
        vec![NPC]
    );
    let mut second = initial.clone();
    second.invocation = [8; 16];
    second.stage = 0;
    second.logical_now = 1.0;
    second.next_order = 2;
    second.scheduled[0].order = 2;
    second.scheduled[0].due = 3.0;
    let next = stage_v3("head-second", 0, 2, &second);
    store.npc_stage(&next).await.unwrap();
    let mut stale = initial.clone();
    stale.stage = 1;
    stale.logical_now = 1.0;
    stale.next_order = 2;
    stale.scheduled[0].order = 2;
    stale.scheduled[0].due = 3.0;
    assert!(
        store
            .npc_stage(&stage_v3("head-stale", 1, 2, &stale))
            .await
            .is_err()
    );
    let head = store.npc_source_head(NPC).await.unwrap().unwrap();
    assert_eq!(head.source_version, 2);
    assert_eq!(head.invocation, [8; 16]);
    let page = store.pending_npc_workflows(NPC, None, 16).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].invocation, [8; 16]);
    let rebased = stage_v3("head-rebased", 1, 3, &stale);
    store.npc_stage(&rebased).await.unwrap();
    assert_eq!(
        store.npc_stage(&rebased).await.unwrap(),
        OperationOutcome::AlreadyCommitted
    );
    let mut complete = stale;
    complete.stage = 2;
    complete.completed = true;
    complete.scheduled.clear();
    store
        .npc_stage(&stage_v3("head-complete", 2, 4, &complete))
        .await
        .unwrap();
    assert!(
        store
            .pending_npc_source_ids(None, 16)
            .await
            .unwrap()
            .is_empty()
    );
    let head = store.npc_source_head(NPC).await.unwrap().unwrap();
    assert!(head.completed);
    assert_eq!(head.source_version, 4);
    let mut fresh = initial;
    fresh.invocation = [9; 16];
    fresh.content_generation = [99; 32];
    assert!(
        store
            .npc_stage(&stage_v3("head-reset-counter", 0, 1, &fresh))
            .await
            .is_err()
    );
    store
        .npc_stage(&stage_v3("head-next-incarnation", 0, 5, &fresh))
        .await
        .unwrap();
    assert_eq!(
        store
            .npc_source_head(NPC)
            .await
            .unwrap()
            .unwrap()
            .source_version,
        5
    );
    owner.close().await.unwrap();
    store.close().await;
}
#[tokio::test]
async fn ambiguous_legacy_source_histories_are_not_recovered_in_invocation_order() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    store.bind_random_key(1, [44; 32]).await.unwrap();
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    for invocation in [[7; 16], [8; 16]] {
        let mut value = checkpoint(0x50000001);
        value.invocation = invocation;
        sqlx::query("INSERT INTO npc_workflows(invocation,source_id,version,stage,program_hash,content_generation,key_version,completed,checkpoint) VALUES($1,$2,1,0,$3,$4,1,false,$5)").bind(invocation.as_slice()).bind(i64::from(NPC)).bind(value.program_hash.as_slice()).bind(value.content_generation.as_slice()).bind(value.encode().unwrap()).execute(&pool).await.unwrap();
    }
    assert_eq!(
        store.pending_npc_source_ids(None, 16).await.unwrap(),
        vec![NPC]
    );
    assert!(store.npc_source_head(NPC).await.is_err());
    assert!(store.pending_npc_workflows(NPC, None, 16).await.is_err());
    assert!(store.pending_npc_source_ids(None, 257).await.is_err());
    pool.close().await;
    store.close().await;
}

#[tokio::test]
async fn completed_archived_source_stays_suppressed_after_continuation_finishes() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let owner = store.acquire_world_owner().await.unwrap();
    store.bind_random_key(1, [44; 32]).await.unwrap();
    let initial = checkpoint(0x50000001);
    store
        .npc_stage(&stage_v3("archive-start", 0, 1, &initial))
        .await
        .unwrap();
    let mut completed = initial;
    completed.stage = 1;
    completed.completed = true;
    completed.scheduled.clear();
    let mut operation = stage_v3("archive-finished", 1, 2, &completed);
    let mut frozen = NpcWorkflowSaveV3::decode(&operation.workflow.checkpoint).unwrap();
    frozen.archive = Some(NpcSourceArchiveV3 {
        source: NPC,
        player: false,
        creature: true,
        cell: 1,
        position: [2., 3., 4.],
        heading: 0.,
        property_revision: 8,
        properties: vec![],
    });
    operation.workflow.checkpoint = frozen.encode().unwrap();
    store.npc_stage(&operation).await.unwrap();
    assert!(
        store
            .pending_npc_source_ids(None, 16)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.suppressed_npc_source_ids(None, 16).await.unwrap(),
        vec![NPC]
    );
    assert!(
        store
            .suppressed_npc_source_ids(Some(NPC), 16)
            .await
            .unwrap()
            .is_empty()
    );
    let recovered = store.npc_source_head(NPC).await.unwrap().unwrap();
    assert!(recovered.completed);
    assert_eq!(recovered.source_version, 2);
    assert!(
        NpcWorkflowSaveV3::decode(&recovered.checkpoint)
            .unwrap()
            .archive
            .is_some()
    );
    owner.close().await.unwrap();
    store.close().await;
}
