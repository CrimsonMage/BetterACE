use bace_content::{Property, Skill};
use bace_dat::{
    CharGen, CreationSkill, HeritageGroup, SkillBase, SkillFormula, SkillTable, XpTable,
};
use bace_gameplay_api::TrainSkill;
use bace_runtime::{character_assets::prepare_character_assets, progression_saves::*};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};
use std::collections::BTreeMap;
fn fixture() -> (XpTable, SkillTable, CharGen) {
    // Synthetic cumulative tables already exercised by bace-character's
    // independent official C# progression oracle, not invented runtime defaults.
    let xp = XpTable {
        attribute_xp: vec![0, 10, 30, 30, 100],
        vital_xp: vec![0, 2, 8, 20, 100],
        trained_skill_xp: vec![0, 5, 15, 50, 100],
        specialized_skill_xp: vec![0, 1, 7, 40, 100],
        character_level_xp: vec![0, 1000, 3000],
        character_level_skill_credits: vec![0, 1, 2],
    };
    let skill = |trained, total| SkillBase {
        description: "retained description".into(),
        name: "synthetic".into(),
        icon_id: 123,
        trained_cost: trained,
        specialized_cost: total,
        category: 1,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 1,
            x: 1,
            y: 0,
            z: 2,
            attribute1: 2,
            attribute2: 4,
        },
        upper_bound: 1.0,
        lower_bound: 0.0,
        learn_modifier: 0.03,
    };
    let skills = SkillTable {
        bucket_size: 17,
        skills: BTreeMap::from([(6, skill(6, 18)), (7, skill(4, 10))]),
    };
    let heritage = HeritageGroup {
        name: "synthetic heritage".into(),
        icon: 42,
        setup: 43,
        environment_setup: 44,
        attribute_credits: 330,
        skill_credits: 52,
        primary_start_areas: vec![],
        secondary_start_areas: vec![],
        skills: vec![CreationSkill {
            skill: 6,
            normal_cost: 2,
            primary_cost: 3,
        }],
        templates: vec![],
        gender_marker: 1,
        genders: BTreeMap::new(),
    };
    let chargen = CharGen {
        reserved: 77,
        starter_areas: vec![],
        heritage_marker: 1,
        heritage_groups: BTreeMap::from([(1, heritage)]),
    };
    (xp, skills, chargen)
}

fn saved() -> PlayerSaveV6 {
    let mut state = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "human".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.ints = vec![
        Property { id: 24, value: 20 },
        Property { id: 188, value: 1 },
        Property {
            id: 9999,
            value: 73,
        },
    ];
    state.properties.int64s = vec![
        Property { id: 2, value: 1000 },
        Property { id: 1, value: 5000 },
    ];
    state.properties.skills = vec![
        Property {
            id: 6,
            value: Skill {
                level_from_pp: 0,
                sac: 1,
                pp: 0,
                init_level: 0,
                resistance_at_last_check: 23,
                last_used_time: 456.5,
            },
        },
        Property {
            id: 999,
            value: Skill {
                level_from_pp: 0,
                sac: 1,
                pp: 0,
                init_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        },
    ];
    let mut result = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: 0x50000001,
                template_revision: 1,
                mutation_revision: 4,
                state,
            },
            account_id: 1,
            name: "Alice smith".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    result.ui.gameplay_options = vec![1, 2, 3];
    result.player.metadata.options1 = 123;
    result.player.metadata.titles = vec![1, 2];
    result
}
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, PlacementOperation, SaveAck, SaveSnapshot,
};
use bace_runtime::saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker};
use bace_runtime::skill_saves::*;
use bace_simulation::SkillTicket;
use std::sync::{Arc, Mutex};
fn ticket(saved: &PlayerSaveV6) -> SkillTicket {
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let state = restore_progression(saved, &assets).unwrap();
    let proposal = state
        .propose_train_skill(TrainSkill {
            skill: 6,
            quoted_credits: 6,
        })
        .unwrap();
    SkillTicket {
        operation: 1,
        context: bace_gameplay_api::ActionContext {
            session: bace_gameplay_api::SessionId(99),
            account: bace_types::AccountId(1),
            actor: bace_types::EntityId(saved.player.entity.object_id),
            sequence: 2,
        },
        expected_revision: proposal.expected_revision(),
        change: proposal.change(),
    }
}
fn lease() -> CharacterLease {
    CharacterLease {
        character_id: 0x50000001,
        epoch: 7,
        state: OwnershipState::Online,
    }
}
#[derive(Clone)]
struct Backend {
    requests: Arc<Mutex<Vec<PlacementOperation>>>,
    results: Arc<Mutex<std::collections::VecDeque<u8>>>,
}
impl SaveBackend for Backend {
    async fn placement(
        &self,
        operation: &PlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.requests.lock().unwrap().push(operation.clone());
        match self.results.lock().unwrap().pop_front().unwrap_or(0) {
            1 => Err(SaveFailure::Timeout),
            2 => Ok(OperationOutcome::AlreadyCommitted),
            3 => Err(SaveFailure::Storage {
                message: "CAS rejected".into(),
                uncertain: false,
            }),
            4 => Ok(OperationOutcome::Committed(vec![])),
            _ => Ok(OperationOutcome::Committed(
                operation
                    .snapshots
                    .iter()
                    .map(|s| SaveAck {
                        object_id: s.object_id,
                        mutation_revision: s.mutation_revision,
                        persisted_version: s.expected_version + 1,
                    })
                    .collect(),
            )),
        }
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("skill must use fenced placement")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("skill must not use unfenced valuable")
    }
}
fn backend(results: impl IntoIterator<Item = u8>) -> Backend {
    Backend {
        requests: Default::default(),
        results: Arc::new(Mutex::new(results.into_iter().collect())),
    }
}
async fn outcome(pending: &mut PendingSkillSave) -> SkillSaveResolution {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(v) = pending.poll() {
                return v;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn uncertainty_retains_exact_snapshot_and_id_until_journal_receipt() {
    let mut saved = saved();
    saved.previous.rares = Some(bace_storage_codec::RareStateV1 {
        character: saved.player.entity.object_id,
        random_identity: [8; 16],
        key_version: 1,
        attempt_ordinal: 7,
        timer_ordinal: 3,
        next_realtime_at: None,
        last_effective_time: 10,
    });
    let ticket = ticket(&saved);
    let mut pending = freeze_skill_ticket(
        SkillOperationId::new([4; 16]).unwrap(),
        ticket,
        &saved,
        9,
        lease(),
    )
    .unwrap();
    let b = backend([1, 2]);
    let worker = spawn_save_worker(b.clone(), SaveWorkerConfig::default()).unwrap();
    pending.submit(&worker.handle).unwrap();
    assert!(pending.submit(&worker.handle).is_err());
    assert!(matches!(
        outcome(&mut pending).await,
        SkillSaveResolution::Uncertain { .. }
    ));
    pending.submit(&worker.handle).unwrap();
    let SkillSaveResolution::Committed {
        owner,
        acknowledgments,
        inventory,
    } = outcome(&mut pending).await
    else {
        panic!()
    };
    assert_eq!(owner, SkillSaveOwner::Plain(ticket));
    assert!(inventory.is_none());
    assert_eq!(acknowledgments[0].persisted_version, 10);
    assert!(pending.submit(&worker.handle).is_err());
    assert!(pending.poll().is_none());
    {
        let requests = b.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].operation_id, requests[1].operation_id);
        assert_eq!(requests[0].snapshots, requests[1].snapshots);
        assert_eq!(requests[0].participants, requests[1].participants);
        assert_eq!(requests[0].leases, requests[1].leases);
        assert_eq!(requests[0].leases, vec![lease()]);
        let loaded = PlayerSaveV6::decode(&requests[0].snapshots[0].bytes).unwrap();
        assert_eq!(loaded.ui, saved.ui);
        assert_eq!(loaded.rares, saved.rares);
        assert_eq!(loaded.enchantments, saved.enchantments);
        assert_eq!(loaded.player.entity.mutation_revision, 5);
    }
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn wrong_receipt_keeps_owner_and_definite_rejection_permits_rollback() {
    let saved = saved();
    let ticket = ticket(&saved);
    let mut pending = freeze_skill_ticket(
        SkillOperationId::new([5; 16]).unwrap(),
        ticket,
        &saved,
        1,
        lease(),
    )
    .unwrap();
    let b = backend([4, 3, 2, 3]);
    let worker = spawn_save_worker(b, SaveWorkerConfig::default()).unwrap();
    pending.submit(&worker.handle).unwrap();
    assert!(matches!(
        outcome(&mut pending).await,
        SkillSaveResolution::Uncertain { .. }
    ));
    pending.submit(&worker.handle).unwrap();
    assert!(matches!(
        outcome(&mut pending).await,
        SkillSaveResolution::Uncertain { .. }
    ));
    pending.submit(&worker.handle).unwrap();
    assert!(matches!(
        outcome(&mut pending).await,
        SkillSaveResolution::Committed { .. }
    ));
    let mut rejected = freeze_skill_ticket(
        SkillOperationId::new([6; 16]).unwrap(),
        ticket,
        &saved,
        1,
        lease(),
    )
    .unwrap();
    rejected.submit(&worker.handle).unwrap();
    assert!(
        matches!(outcome(&mut rejected).await, SkillSaveResolution::Rejected { owner: SkillSaveOwner::Plain(t), .. } if t == ticket)
    );
    assert!(pending.submit(&worker.handle).is_err());
    worker.handle.close();
    worker.task.await.unwrap();
}
#[test]
fn invalid_identity_lease_revision_and_version_rejected_before_submission() {
    let saved = saved();
    let t = ticket(&saved);
    let id = SkillOperationId::new([3; 16]).unwrap();
    assert!(SkillOperationId::new([0; 16]).is_err());
    for version in [0, -1, i64::MAX] {
        assert!(freeze_skill_ticket(id, t, &saved, version, lease()).is_err());
    }
    let mut wrong = lease();
    wrong.epoch = 0;
    assert!(freeze_skill_ticket(id, t, &saved, 1, wrong).is_err());
    let mut wrong = lease();
    wrong.state = OwnershipState::Offline;
    assert!(freeze_skill_ticket(id, t, &saved, 1, wrong).is_err());
    let mut t = t;
    t.expected_revision += 1;
    assert!(freeze_skill_ticket(id, t, &saved, 1, lease()).is_err());
}
#[tokio::test]
async fn composite_consumption_and_skill_write_share_one_fenced_receipt() {
    use bace_inventory::*;
    use bace_runtime::game_inventory::{FrozenInventoryItem, InventoryFreezeInput};
    use bace_storage_codec::ItemPlacementV2;
    use bace_types::EntityId;
    let saved = saved();
    let skill = ticket(&saved);
    let actor = skill.context.actor;
    let before = InventoryItem {
        structure: None,
        id: EntityId(42),
        revision: 2,
        template: 1,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: actor,
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    let mut after = before.clone();
    after.revision += 1;
    after.stack = 0;
    after.place = ItemPlace::Removed;
    let proposal = InventoryProposal {
        changes: vec![ItemChange {
            before: Some(before),
            after,
        }],
        participants: vec![(actor, 4), (EntityId(42), 2)],
        actor_burden: 0,
        requires_pickup_motion: false,
    };
    let registry = bace_magic::EnchantmentRegistry::new(4096).unwrap();
    let cooldown_proposal = registry.propose_cooldown(7, 42, 30.0).unwrap();
    let cooldown_after = registry.preview(&cooldown_proposal).unwrap();
    let device = bace_simulation::SkillDeviceTicket {
        skill,
        cooldown: Some(bace_simulation::SkillDeviceCooldown {
            group: 7,
            seconds: 30.0,
            before_revision: registry.revision(),
            after_revision: registry.revision() + 1,
            after: cooldown_after,
        }),
        inventory: bace_simulation::InventoryTicket {
            operation: 22,
            actor,
            proposal,
        },
    };
    let mut entity = saved.player.entity.clone();
    entity.object_id = 42;
    entity.mutation_revision = 2;
    let items = vec![FrozenInventoryItem {
        corpse: None,
        construction: None,
        enchantments: vec![],
        source_destination: Some(2),
        entity,
        placement: Some(ItemPlacementV2::Contained {
            container: actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }),
        persisted_version: 4,
    }];
    let positions = std::collections::BTreeMap::new();
    let leases = [lease()];
    let input = InventoryFreezeInput {
        operation_id: "ignored-local-counter",
        proposal: &device.inventory.proposal,
        items: &items,
        other_snapshots: &[],
        leases: &leases,
        storage_views: &[],
        admitted_positions: &positions,
    };
    let mut pending = freeze_skill_device(
        SkillOperationId::new([9; 16]).unwrap(),
        device.clone(),
        &saved,
        9,
        lease(),
        input,
    )
    .unwrap();
    assert_eq!(pending.operation().snapshots.len(), 2);
    assert_eq!(pending.operation().changes.len(), 1);
    let item = bace_storage_codec::ItemSaveV5::decode(
        &pending
            .operation()
            .snapshots
            .iter()
            .find(|snapshot| snapshot.object_id == 42)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(item.source_destination, Some(2));
    let persisted_player = bace_storage_codec::PlayerSaveV6::decode(
        &pending
            .operation()
            .snapshots
            .iter()
            .find(|snapshot| snapshot.object_id == actor.0)
            .unwrap()
            .bytes,
    )
    .unwrap();
    let restored = bace_runtime::enchantment_saves::restore_saved_enchantments(
        &persisted_player,
        4096,
        |_| None,
    )
    .unwrap();
    assert_eq!(restored.entries()[0].spell, 0x8007);
    assert_eq!(restored.entries()[0].spec.duration, 30.0);
    let projection = bace_runtime::enchantment_saves::prepare_enchantment_projection(
        &restored.entries()[0],
        None,
    )
    .unwrap();
    assert_eq!(
        bace_replication::project_enchantments(&[projection], 1)
            .unwrap()
            .cooldown
            .len(),
        1
    );
    let b = backend([0]);
    let worker = spawn_save_worker(b, SaveWorkerConfig::default()).unwrap();
    pending.submit(&worker.handle).unwrap();
    let SkillSaveResolution::Committed {
        owner,
        inventory: Some(receipt),
        acknowledgments,
    } = outcome(&mut pending).await
    else {
        panic!()
    };
    assert_eq!(owner, SkillSaveOwner::Device(device));
    assert_eq!(receipt.operation, 22);
    assert_eq!(receipt.revisions, vec![(EntityId(42), 3)]);
    assert_eq!(acknowledgments.len(), 2);
    worker.handle.close();
    worker.task.await.unwrap();
}

#[tokio::test]
async fn attribute_transfer_freezes_two_starting_values_and_device_in_one_receipt() {
    use bace_character::{
        CharacterProgression, ProgressionTables, RankTable, TraitProgress, TraitState,
    };
    use bace_gameplay_api::{AttributeId, ProgressionTarget, SkillAdvancement, TraitDetails};
    use bace_inventory::*;
    use bace_runtime::game_inventory::{FrozenInventoryItem, InventoryFreezeInput};
    use bace_storage_codec::ItemPlacementV2;
    use bace_types::EntityId;
    let mut saved = saved();
    saved.player.entity.state.properties.attributes = vec![
        Property {
            id: 1,
            value: bace_content::Attribute {
                init_level: 50,
                level_from_cp: 0,
                cp_spent: 0,
            },
        },
        Property {
            id: 2,
            value: bace_content::Attribute {
                init_level: 97,
                level_from_cp: 0,
                cp_spent: 0,
            },
        },
    ];
    let table = RankTable::new(&[0, 100]).unwrap();
    let character = CharacterProgression::with_state(
        &[
            TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Attribute(AttributeId::Strength),
                    experience_spent: 0,
                    advancement: SkillAdvancement::Inactive,
                },
                details: TraitDetails::Attribute { starting_value: 50 },
            },
            TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Attribute(AttributeId::Endurance),
                    experience_spent: 0,
                    advancement: SkillAdvancement::Inactive,
                },
                details: TraitDetails::Attribute { starting_value: 97 },
            },
        ],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        1000,
        4,
    )
    .unwrap();
    let proposal = character
        .propose_attribute_transfer(AttributeId::Strength, AttributeId::Endurance, false)
        .unwrap();
    let actor = EntityId(saved.player.entity.object_id);
    let before = InventoryItem {
        structure: None,
        id: EntityId(42),
        revision: 2,
        template: 1,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: actor,
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    let mut after = before.clone();
    after.revision += 1;
    after.stack = 0;
    after.place = ItemPlace::Removed;
    let ticket = bace_simulation::AttributeTransferDeviceTicket {
        character: bace_simulation::AttributeTransferTicket {
            operation: 1,
            context: bace_gameplay_api::ActionContext {
                actor,
                account: bace_types::AccountId(1),
                session: bace_gameplay_api::SessionId(99),
                sequence: 2,
            },
            proposal,
        },
        inventory: bace_simulation::InventoryTicket {
            operation: 22,
            actor,
            proposal: InventoryProposal {
                changes: vec![ItemChange {
                    before: Some(before),
                    after,
                }],
                participants: vec![(actor, 4), (EntityId(42), 2)],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
    };
    let mut entity = saved.player.entity.clone();
    entity.object_id = 42;
    entity.mutation_revision = 2;
    let items = vec![FrozenInventoryItem {
        corpse: None,
        construction: None,
        enchantments: vec![],
        source_destination: Some(2),
        entity,
        placement: Some(ItemPlacementV2::Contained {
            container: actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }),
        persisted_version: 4,
    }];
    let leases = [lease()];
    let positions = BTreeMap::new();
    let mut pending = freeze_attribute_transfer_device(
        SkillOperationId::new([0x2a; 16]).unwrap(),
        ticket.clone(),
        &saved,
        9,
        lease(),
        InventoryFreezeInput {
            operation_id: "replaced",
            proposal: &ticket.inventory.proposal,
            items: &items,
            other_snapshots: &[],
            leases: &leases,
            storage_views: &[],
            admitted_positions: &positions,
        },
    )
    .unwrap();
    assert_eq!(pending.operation().snapshots.len(), 2);
    assert_eq!(pending.operation().changes.len(), 1);
    let item = bace_storage_codec::ItemSaveV5::decode(
        &pending
            .operation()
            .snapshots
            .iter()
            .find(|snapshot| snapshot.object_id == 42)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(item.source_destination, Some(2));
    assert!(
        pending
            .operation()
            .operation_id
            .starts_with("attribute-transfer:")
    );
    let player = pending
        .operation()
        .snapshots
        .iter()
        .find(|snapshot| snapshot.object_id == actor.0)
        .unwrap();
    let frozen = PlayerSaveV6::decode(&player.bytes).unwrap();
    assert_eq!(frozen.player.entity.mutation_revision, 5);
    assert_eq!(frozen.ui, saved.ui);
    assert_eq!(
        frozen
            .player
            .entity
            .state
            .properties
            .attributes
            .iter()
            .map(|attribute| attribute.value.init_level)
            .collect::<Vec<_>>(),
        [47, 100]
    );
    let worker = spawn_save_worker(backend([0]), SaveWorkerConfig::default()).unwrap();
    pending.submit(&worker.handle).unwrap();
    let SkillSaveResolution::Committed {
        owner,
        inventory: Some(receipt),
        acknowledgments,
    } = outcome(&mut pending).await
    else {
        panic!("exact attribute transfer receipt")
    };
    assert_eq!(owner, SkillSaveOwner::AttributeTransfer(ticket));
    assert_eq!(receipt.operation, 22);
    assert_eq!(receipt.revisions, vec![(EntityId(42), 3)]);
    assert_eq!(acknowledgments.len(), 2);
    worker.handle.close();
    worker.task.await.unwrap();
}
