use bace_content::{Property, WeenieV1};
use bace_entity::{PropertyChange, PropertyFamily, PropertyValue};
use bace_runtime::npc_persistence::freeze_player_effect;
use bace_simulation::{NpcAggregateFence, NpcEffect};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use bace_types::EntityId;
fn saved() -> PlayerSaveV6 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "human".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.int64s = vec![
        Property { id: 1, value: 100 },
        Property { id: 2, value: 30 },
    ];
    state.properties.ints = vec![
        Property { id: 25, value: 1 },
        Property {
            id: 9999,
            value: 73,
        },
    ];
    PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 7,
            mutation_revision: 4,
            state,
        },
        account_id: 2,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}

fn service_checkpoint(
    proposal: bace_simulation::NpcProposal,
) -> bace_simulation::NpcSourceCheckpoint {
    bace_simulation::NpcSourceCheckpoint {
        inventory: None,
        location: None,
        properties: None,
        source_quests: None,
        archive: None,
        source: proposal.context.source,
        active_operation: 1,
        invocations: vec![bace_simulation::NpcInvocationCheckpoint {
            operation: 1,
            event_id: [4; 16],
            key_version: 1,
            random_position: 1,
        }],
        logical_now: 1.0,
        event_id: [4; 16],
        key_version: 1,
        random_position: 1,
        vm: bace_emotes::NativeCheckpoint {
            order: 1,
            remaining: 100,
            work: vec![],
            pending: vec![bace_emotes::NativePendingRow {
                ticket: proposal.ticket,
                row: bace_emotes::NativeScheduledRow {
                    inline: true,
                    depth: 0,
                    set: 0,
                    action: 0,
                    due: 1.0,
                    order: 1,
                    context: proposal.context,
                },
            }],
            detached: vec![],
        },
        pending: vec![bace_simulation::NpcPendingCheckpoint {
            proposal,
            completion: bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
            adopted: true,
            detached: false,
        }],
    }
}
#[test]
fn live_source_stage_freezes_properties_and_quests_without_a_player_save() {
    use bace_runtime::npc_persistence::{
        NpcArchiveStageInput, NpcCheckpointBinding, freeze_archive_stage, restore_checkpoint,
    };
    let mut properties = bace_entity::EntityProperties::new(4096).unwrap();
    let change = properties
        .propose(PropertyFamily::Int, 999, Some(PropertyValue::Int(7)))
        .unwrap();
    properties.adopt(change.clone()).unwrap();
    let proposal = bace_simulation::NpcProposal {
        ticket: 9,
        context: bace_gameplay_api::NpcContext {
            source: EntityId(2),
            target: Some(EntityId(2)),
            operation: 1,
        },
        effect: NpcEffect::Property {
            actor: EntityId(2),
            aggregate: None,
            change,
        },
    };
    let mut checkpoint = service_checkpoint(proposal.clone());
    checkpoint.properties = Some(properties);
    checkpoint.source_quests = Some((
        3,
        vec![(
            "FLAG".into(),
            bace_quests::QuestProgress {
                last_completed_seconds: 23,
                completions: -2,
            },
        )],
    ));
    let binding = NpcCheckpointBinding {
        source_version: 0,
        source_template: 2,
        source: 2,
        invocation: [4; 16],
        program_hash: [1; 32],
        content_generation: [2; 32],
    };
    let pending = freeze_archive_stage(NpcArchiveStageInput {
        binding,
        world_epoch: 1,
        workflow_version: 0,
        proposal: proposal.clone(),
        committed_checkpoint: checkpoint.clone(),
        leases: vec![],
    })
    .unwrap();
    assert!(pending.operation().inventory.snapshots.is_empty());
    let decoded =
        bace_storage_codec::NpcWorkflowSaveV3::decode(&pending.operation().workflow.checkpoint)
            .unwrap();
    let restored = restore_checkpoint(binding, decoded).unwrap();
    assert_eq!(restored.properties, checkpoint.properties);
    assert_eq!(restored.source_quests, checkpoint.source_quests);
    checkpoint.properties = Some(bace_entity::EntityProperties::new(4096).unwrap());
    assert!(
        freeze_archive_stage(NpcArchiveStageInput {
            binding,
            world_epoch: 1,
            workflow_version: 0,
            proposal,
            committed_checkpoint: checkpoint,
            leases: vec![]
        })
        .is_err()
    );
}
#[test]
fn teach_spell_freezes_canonical_book_and_preserves_full_player_supplements() {
    use bace_runtime::npc_persistence::{
        NpcCheckpointBinding, NpcSpellbookStageInput, freeze_spellbook_stage,
    };
    let player = saved();
    let proposal = bace_simulation::NpcProposal {
        ticket: 3,
        context: bace_gameplay_api::NpcContext {
            source: EntityId(0x80000002),
            target: Some(EntityId(0x50000001)),
            operation: 1,
        },
        effect: NpcEffect::Service(bace_gameplay_api::NpcOperation::Reward {
            kind: bace_gameplay_api::NpcRewardKind::TeachSpell,
            amount: 100,
            stat: None,
            percent: 0.0,
            minimum: 0,
            maximum: 0,
        }),
    };
    let ticket = bace_simulation::NpcSpellbookTicket {
        npc: proposal.clone(),
        actor: EntityId(0x50000001),
        spell: 100,
        before_revision: 4,
        after_revision: 5,
        before: vec![],
        after: vec![100],
    };
    let pending = freeze_spellbook_stage(NpcSpellbookStageInput {
        binding: NpcCheckpointBinding {
            source_version: 0,
            source_template: 100,
            invocation: [1; 16],
            source: 0x80000002,
            program_hash: [2; 32],
            content_generation: [3; 32],
        },
        world_epoch: 7,
        workflow_version: 0,
        ticket: &ticket,
        committed_checkpoint: service_checkpoint(proposal),
        player: &player,
        player_version: 4,
        lease: bace_persistence::CharacterLease {
            character_id: 0x50000001,
            epoch: 3,
            state: bace_persistence::OwnershipState::Online,
        },
    })
    .unwrap();
    let after = PlayerSaveV6::decode(&pending.operation().inventory.snapshots[0].bytes).unwrap();
    assert_eq!(
        after.player.entity.state.properties.spell_book,
        vec![Property {
            id: 100,
            value: 1.0
        }]
    );
    assert_eq!(
        after.player.entity.state.properties.ints,
        player.player.entity.state.properties.ints
    );
    assert_eq!(after.ui, player.ui);
    assert_eq!(after.physical_recovery, player.physical_recovery);
    assert_eq!(after.player.entity.mutation_revision, 5);
}
#[test]
fn signed_training_credit_stage_freezes_both_nullable_counters() {
    use bace_runtime::npc_persistence::{
        NpcCheckpointBinding, NpcTrainingCreditStageInput, freeze_training_credit_stage,
    };
    let mut player = saved();
    player.player.entity.state.properties.ints.extend([
        Property { id: 23, value: 10 },
        Property { id: 24, value: 5 },
    ]);
    player
        .player
        .entity
        .state
        .properties
        .ints
        .sort_by_key(|p| p.id);
    let proposal = bace_simulation::NpcProposal {
        ticket: 3,
        context: bace_gameplay_api::NpcContext {
            source: EntityId(0x80000002),
            target: Some(EntityId(0x50000001)),
            operation: 1,
        },
        effect: NpcEffect::Service(bace_gameplay_api::NpcOperation::Reward {
            kind: bace_gameplay_api::NpcRewardKind::TrainingCredits,
            amount: -3,
            stat: None,
            percent: 0.0,
            minimum: 0,
            maximum: 0,
        }),
    };
    let ticket = bace_simulation::NpcTrainingCreditTicket {
        npc: proposal.clone(),
        actor: EntityId(0x50000001),
        change: bace_character::TrainingCreditChange {
            before_revision: 4,
            after_revision: 5,
            amount: -3,
            before_available: Some(5),
            after_available: Some(2),
            before_total: Some(10),
            after_total: Some(7),
        },
    };
    let pending = freeze_training_credit_stage(NpcTrainingCreditStageInput {
        binding: NpcCheckpointBinding {
            source_version: 0,
            source_template: 100,
            invocation: [1; 16],
            source: 0x80000002,
            program_hash: [2; 32],
            content_generation: [3; 32],
        },
        world_epoch: 7,
        workflow_version: 0,
        ticket: &ticket,
        committed_checkpoint: service_checkpoint(proposal),
        player: &player,
        player_version: 4,
        lease: bace_persistence::CharacterLease {
            character_id: 0x50000001,
            epoch: 3,
            state: bace_persistence::OwnershipState::Online,
        },
    })
    .unwrap();
    let after = PlayerSaveV6::decode(&pending.operation().inventory.snapshots[0].bytes).unwrap();
    let fields = &after.player.entity.state.properties.ints;
    assert_eq!(fields.iter().find(|p| p.id == 23).unwrap().value, 7);
    assert_eq!(fields.iter().find(|p| p.id == 24).unwrap().value, 2);
    assert_eq!(fields.iter().find(|p| p.id == 9999).unwrap().value, 73);
    assert_eq!(after.player.entity.mutation_revision, 5);
}
#[test]
fn character_aggregate_fence_is_distinct_from_local_property_revision() {
    let saved = saved();
    let mut effect = NpcEffect::Property {
        actor: EntityId(0x50000001),
        aggregate: Some(NpcAggregateFence {
            before_revision: 4,
            after_revision: 5,
        }),
        change: PropertyChange {
            family: PropertyFamily::Int,
            stat: 9999,
            before: Some(PropertyValue::Int(73)),
            after: Some(PropertyValue::Int(74)),
            before_revision: 900,
            after_revision: 901,
        },
    };
    let next = freeze_player_effect(&saved, &effect).unwrap();
    assert_eq!(next.player.entity.mutation_revision, 5);
    assert_eq!(next.player.entity.template_revision, 7);
    assert_eq!(
        next.player.entity.state.properties.int64s,
        saved.player.entity.state.properties.int64s
    );
    assert_eq!(
        saved
            .player
            .entity
            .state
            .properties
            .ints
            .last()
            .unwrap()
            .value,
        73
    );
    if let NpcEffect::Property { aggregate, .. } = &mut effect {
        *aggregate = Some(NpcAggregateFence {
            before_revision: 3,
            after_revision: 4,
        });
    }
    assert!(freeze_player_effect(&saved, &effect).is_err());
}
#[test]
fn completed_quest_stage_preserves_high_bit_flags_and_survives_a_later_stage_rejection() {
    let saved = saved();
    let progress = bace_quests::QuestProgress {
        last_completed_seconds: u32::MAX,
        completions: i32::MIN,
    };
    let stage = NpcEffect::Quest {
        actor: EntityId(0x50000001),
        aggregate: Some(NpcAggregateFence {
            before_revision: 4,
            after_revision: 5,
        }),
        change: bace_quests::QuestChange {
            name: "flagquest".into(),
            before: None,
            after: Some(progress),
            before_revision: 100,
            after_revision: 101,
        },
    };
    let committed = freeze_player_effect(&saved, &stage).unwrap();
    let bytes = committed.encode().unwrap();
    let restored = PlayerSaveV6::decode_or_migrate(&bytes).unwrap();
    assert_eq!(restored.player.quests[0].completions, 0x80000000);
    assert_eq!(
        restored.player.quests[0].last_completed,
        i64::from(u32::MAX)
    );
    let stale_reward = NpcEffect::Experience {
        actor: EntityId(0x50000001),
        credit: bace_character::ExperienceCredit {
            before_revision: 4,
            after_revision: 5,
            before_available: 30,
            after_available: 50,
        },
    };
    assert!(freeze_player_effect(&restored, &stale_reward).is_err());
    assert_eq!(restored.encode().unwrap(), bytes);
    let reward = NpcEffect::Experience {
        actor: EntityId(0x50000001),
        credit: bace_character::ExperienceCredit {
            before_revision: 5,
            after_revision: 6,
            before_available: 30,
            after_available: 50,
        },
    };
    let after = freeze_player_effect(&restored, &reward).unwrap();
    assert_eq!(after.player.quests, restored.player.quests);
    assert_eq!(after.player.entity.mutation_revision, 6);
}
#[test]
fn title_and_contract_freezers_keep_other_owned_supplements_and_reject_wrong_before_state() {
    let saved = saved();
    let before = bace_runtime::native_player::restore_services(&saved).unwrap();
    let change = before.propose_title(4, 42).unwrap();
    let titled = freeze_player_effect(
        &saved,
        &NpcEffect::CharacterService {
            actor: EntityId(0x50000001),
            change,
        },
    )
    .unwrap();
    assert_eq!(titled.player.metadata.titles, vec![42]);
    let contract = NpcEffect::Contract {
        actor: EntityId(0x50000001),
        before_revision: 5,
        change: bace_quests::ContractChange {
            before: vec![],
            after: vec![bace_quests::ContractState {
                id: 123,
                display: true,
            }],
        },
    };
    let after = freeze_player_effect(&titled, &contract).unwrap();
    assert_eq!(after.player.metadata, titled.player.metadata);
    assert_eq!(after.contracts[0].id, 123);
    assert!(freeze_player_effect(&after, &contract).is_err());
}

#[test]
fn explicit_spend_stage_debits_only_spendable_experience() {
    let saved = saved();
    let effect = NpcEffect::Experience {
        actor: EntityId(0x50000001),
        credit: bace_character::ExperienceCredit {
            before_revision: 4,
            after_revision: 5,
            before_available: 30,
            after_available: 10,
        },
    };
    let after = freeze_player_effect(&saved, &effect).unwrap();
    assert_eq!(
        after
            .player
            .entity
            .state
            .properties
            .int64s
            .iter()
            .find(|p| p.id == 2)
            .unwrap()
            .value,
        10
    );
    assert_eq!(
        after
            .player
            .entity
            .state
            .properties
            .int64s
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .value,
        100
    );
    assert!(freeze_player_effect(&after, &effect).is_err());
}

#[test]
fn awarding_a_title_appends_without_sorting_the_existing_frozen_book() {
    let mut saved = saved();
    saved.player.metadata.titles = vec![99, 3];
    let before = bace_runtime::native_player::restore_services(&saved).unwrap();
    let change = before.propose_title(4, 5).unwrap();
    let after = freeze_player_effect(
        &saved,
        &NpcEffect::CharacterService {
            actor: EntityId(0x50000001),
            change,
        },
    )
    .unwrap();
    assert_eq!(after.player.metadata.titles, vec![99, 3, 5]);
    assert_eq!(saved.player.metadata.titles, vec![99, 3]);
}

#[test]
fn service_save_preserves_absent_zero_and_earned_total_skill_credits() {
    use bace_runtime::native_player::{freeze_services, restore_contracts, restore_services};
    let mut saved = saved();
    let contracts = restore_contracts(&saved).unwrap();
    assert_eq!(restore_services(&saved).unwrap().total_skill_credits, None);
    for credits in [Some(0), Some(88), None] {
        let mut services = restore_services(&saved).unwrap();
        services.total_skill_credits = credits;
        freeze_services(&mut saved, &services, &contracts).unwrap();
        saved = PlayerSaveV6::decode(&saved.encode().unwrap()).unwrap();
        assert_eq!(
            restore_services(&saved).unwrap().total_skill_credits,
            credits
        );
        assert_eq!(
            saved
                .player
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|property| property.id == 23)
                .map(|property| property.value),
            credits
        );
        assert!(
            saved
                .player
                .entity
                .state
                .properties
                .ints
                .iter()
                .any(|property| property.id == 9999 && property.value == 73)
        );
    }
}

#[test]
fn no_op_quest_stage_preserves_unsorted_records_and_canonical_updates_keep_names() {
    let mut saved = saved();
    saved.player.quests = vec![
        bace_storage_codec::QuestSaveV1 {
            name: "z".into(),
            completions: 1,
            last_completed: 10,
        },
        bace_storage_codec::QuestSaveV1 {
            name: "a".into(),
            completions: 2,
            last_completed: 20,
        },
    ];
    let no_op = NpcEffect::Quest {
        actor: EntityId(0x50000001),
        aggregate: Some(NpcAggregateFence {
            before_revision: 4,
            after_revision: 4,
        }),
        change: bace_quests::QuestChange {
            name: "missing".into(),
            before: None,
            after: None,
            before_revision: 7,
            after_revision: 7,
        },
    };
    assert_eq!(freeze_player_effect(&saved, &no_op).unwrap(), saved);
    let update = NpcEffect::Quest {
        actor: EntityId(0x50000001),
        aggregate: Some(NpcAggregateFence {
            before_revision: 4,
            after_revision: 5,
        }),
        change: bace_quests::QuestChange {
            name: "Z".into(),
            before: Some(bace_quests::QuestProgress {
                completions: 1,
                last_completed_seconds: 10,
            }),
            after: Some(bace_quests::QuestProgress {
                completions: 2,
                last_completed_seconds: 10,
            }),
            before_revision: 7,
            after_revision: 8,
        },
    };
    let after = freeze_player_effect(&saved, &update).unwrap();
    assert_eq!(after.player.quests[0].name, "z");
    assert_eq!(after.player.quests[0].completions, 2);
    assert_eq!(after.player.quests[1], saved.player.quests[1]);
}
#[test]
fn earned_level_credit_freezes_both_counters_and_preserves_nullable_total() {
    use bace_character::{
        CharacterLevelTable, CharacterProgression, ProgressionTables, RankTable, SkillTrainingRules,
    };
    use std::sync::Arc;
    for total in [Some(10), None] {
        let mut saved = saved();
        saved
            .player
            .entity
            .state
            .properties
            .ints
            .push(Property { id: 24, value: 2 });
        if let Some(total) = total {
            saved.player.entity.state.properties.ints.push(Property {
                id: 23,
                value: total,
            });
        }
        let ranks = RankTable::new(&[0, 10, 100]).unwrap();
        let progression = CharacterProgression::new(
            &[],
            Arc::new(ProgressionTables {
                attributes: ranks.clone(),
                vitals: ranks.clone(),
                trained_skills: ranks.clone(),
                specialized_skills: ranks,
            }),
            30,
            4,
        )
        .unwrap()
        .with_training(Arc::new(SkillTrainingRules::new(&[]).unwrap()), 2, &[])
        .unwrap();
        let services = bace_runtime::native_player::restore_services(&saved).unwrap();
        let change = progression
            .propose_earned_experience(
                &services,
                &CharacterLevelTable::prepare(vec![0, 0, 200], vec![0, 0, 3]).unwrap(),
                100,
            )
            .unwrap();
        let after = freeze_player_effect(
            &saved,
            &NpcEffect::EarnedExperience {
                actor: EntityId(0x50000001),
                change,
            },
        )
        .unwrap();
        let ints = &after.player.entity.state.properties.ints;
        assert_eq!(ints.iter().find(|p| p.id == 24).unwrap().value, 5);
        assert_eq!(
            ints.iter().find(|p| p.id == 23).map(|p| p.value),
            total.map(|v| v + 3)
        );
        assert_eq!(after.player.entity.mutation_revision, 5);
    }
}
