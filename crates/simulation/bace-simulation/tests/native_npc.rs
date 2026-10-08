#[path = "magic_common/mod.rs"]
#[allow(
    unused_imports,
    dead_code,
    reason = "Shared prepared magic fixture also exposes client-only helpers used by other suites"
)]
mod magic_common;
#[path = "native_npc/source_state.rs"]
mod source_state;
use bace_character::{CharacterProgression, LuminanceState, ProgressionTables, RankTable};
use bace_content::{Emote, EmoteAction};
use bace_emotes::{NativeLimits, NativeProgram, NativeTrigger};
use bace_entity::{
    Actor, Combatant, CombatantProfile, EntityProperties, PropertyFamily, PropertyValue,
};
use bace_gameplay_api::{CharacterBinding, NpcFailure, SessionId};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::{CharacterRegistrationError, Kernel, NpcEffect};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;
use std::sync::Arc;
#[path = "native_npc/archive.rs"]
mod archived_sources;
#[path = "native_npc/casting.rs"]
mod casting_services;
#[path = "native_npc/inventory.rs"]
mod inventory_services;
#[path = "native_npc/level_xp.rs"]
mod level_xp;
#[path = "native_npc/movement.rs"]
mod movement_services;
#[path = "native_npc/spellbook.rs"]
mod spellbook_services;
#[path = "native_npc/training_credits.rs"]
mod training_credit_services;
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    }
}
fn kernel(capacity: usize) -> Kernel {
    kernel_with_training(capacity, false)
}
fn kernel_with_training(capacity: usize, training: bool) -> Kernel {
    kernel_with_turn_rate(capacity, training, 0.0)
}
fn kernel_with_turn_rate(capacity: usize, training: bool, turn_rate: f32) -> Kernel {
    kernel_with_npc_x(capacity, training, turn_rate, 2.0)
}
fn kernel_with_npc_x(capacity: usize, training: bool, turn_rate: f32, npc_x: f32) -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = Body::spawn_oriented(
            world.scene(cell).unwrap(),
            Vec3::new(if id == 2 { npc_x } else { 1.0 }, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 1.0,
                jump_impulse: 1.0,
            },
            0.0,
            turn_rate,
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 1,
                    melee_range: 2.0,
                    attack_duration: 1.0,
                    strike_offsets: vec![0.5],
                    player: id == 1,
                })
                .unwrap()
                .with_resources(
                    Some(bace_entity::VitalPool {
                        current: 10,
                        maximum: 10,
                    }),
                    Some(bace_entity::VitalPool {
                        current: 10,
                        maximum: 10,
                    }),
                )
                .unwrap(),
            )
            .unwrap();
        let mut props = EntityProperties::new(100).unwrap();
        let change = props
            .propose(
                PropertyFamily::String,
                1,
                Some(PropertyValue::String(
                    if id == 1 { "Player" } else { "Keeper" }.into(),
                )),
            )
            .unwrap();
        props.adopt(change).unwrap();
        if id == 2 {
            let change = props
                .propose(PropertyFamily::Bool, 8, Some(PropertyValue::Bool(true)))
                .unwrap();
            props.adopt(change).unwrap();
        }
        world.register_properties(EntityId(id), props).unwrap();
    }
    let table = RankTable::new(&[0, 10, 100]).unwrap();
    let character = CharacterProgression::new(
        &[],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        0,
    )
    .unwrap()
    .with_luminance(LuminanceState {
        available: 0,
        maximum: 1000,
    })
    .unwrap();
    let character = if training {
        character
            .with_training(
                Arc::new(bace_character::SkillTrainingRules::new(&[]).unwrap()),
                5,
                &[],
            )
            .unwrap()
    } else {
        character
    };
    let mut kernel = Kernel::new(world, capacity).unwrap();
    kernel.register_character(binding(), character).unwrap();
    kernel
        .register_quest_registry(EntityId(1), bace_quests::QuestRegistry::new(100).unwrap())
        .unwrap();
    kernel
        .configure_npc_services(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
            vec![(
                "flag".into(),
                bace_quests::QuestDefinition::new("flag", 30, -1),
            )],
            bace_world_events::Events::prepare(vec![], false).unwrap(),
        )
        .unwrap();
    kernel
}
fn set(category: i32, key: Option<&str>, actions: Vec<EmoteAction>) -> Emote {
    Emote {
        category,
        quest: key.map(str::to_owned),
        probability: 1.0,
        actions,
        ..Default::default()
    }
}
#[test]
fn signed_portal_use_radius_preserves_separated_denial_and_overlap_use() {
    // Pinned ACE WorldObject_Use.IsWithinUseRadiusOf compares a signed
    // CylinderDistance with the authored Float54. WCIDs 29338 and 31061 in
    // Patches v0.9.295 use -0.1 and carry Portal-category emotes.
    let program = Arc::new(
        NativeProgram::prepare(
            vec![
                set(
                    7,
                    None,
                    vec![EmoteAction {
                        r#type: 8,
                        message: Some("used".into()),
                        ..Default::default()
                    }],
                ),
                set(
                    4,
                    None,
                    vec![EmoteAction {
                        r#type: 8,
                        message: Some("entered".into()),
                        ..Default::default()
                    }],
                ),
            ],
            NativeLimits::default(),
        )
        .unwrap(),
    );
    let context = |sequence| bace_gameplay_api::ActionContext {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
        sequence,
    };
    let mut separated = kernel(8);
    separated
        .register_native_npc(EntityId(2), program.clone(), -0.1)
        .unwrap();
    assert_eq!(
        separated.use_native_npc(context(1), EntityId(2), [1; 16], 1),
        Err(NpcFailure::InvalidInput)
    );
    assert_eq!(
        separated.start_npc_emote(
            EntityId(2),
            Some(EntityId(1)),
            NativeTrigger {
                category: 4,
                ..Default::default()
            },
            [2; 16],
            2,
            false,
        ),
        Ok(true)
    );

    let mut overlapping = kernel_with_npc_x(8, false, 0.0, 1.5);
    overlapping
        .register_native_npc(EntityId(2), program, -0.1)
        .unwrap();
    assert_eq!(
        overlapping.use_native_npc(context(1), EntityId(2), [3; 16], 3),
        Ok(true)
    );
}
fn install(kernel: &mut Kernel, sets: Vec<Emote>) {
    kernel
        .register_native_npc(
            EntityId(2),
            Arc::new(NativeProgram::prepare(sets, NativeLimits::default()).unwrap()),
            3.0,
        )
        .unwrap();
    kernel
        .start_npc_emote(
            EntityId(2),
            Some(EntityId(1)),
            NativeTrigger {
                category: 7,
                ..Default::default()
            },
            [9; 16],
            3,
            true,
        )
        .unwrap();
}
#[test]
fn native_actions_change_actual_property_quest_xp_spend_luminance_only_after_exact_receipts() {
    let mut k = kernel(8);
    install(
        &mut k,
        vec![
            set(
                7,
                None,
                vec![
                    EmoteAction {
                        r#type: 53,
                        stat: Some(999),
                        amount: Some(42),
                        ..Default::default()
                    },
                    EmoteAction {
                        r#type: 36,
                        stat: Some(999),
                        min: Some(42),
                        max: Some(42),
                        message: Some("correct".into()),
                        ..Default::default()
                    },
                    EmoteAction {
                        r#type: 2,
                        amount: Some(-50),
                        ..Default::default()
                    },
                    EmoteAction {
                        r#type: 113,
                        amount64: Some(1200),
                        ..Default::default()
                    },
                    EmoteAction {
                        r#type: 22,
                        message: Some("flag".into()),
                        ..Default::default()
                    },
                ],
            ),
            set(
                22,
                Some("correct"),
                vec![EmoteAction {
                    r#type: 8,
                    message: Some("%n greets %s".into()),
                    ..Default::default()
                }],
            ),
        ],
    );
    assert!(k.step().unwrap().is_empty());
    let proposal = k.take_npc_proposal().unwrap();
    assert!(matches!(proposal.effect, NpcEffect::Property { .. }));
    assert_eq!(
        k.world()
            .properties(EntityId(1))
            .unwrap()
            .get(PropertyFamily::Int, 999),
        None
    );
    assert_eq!(
        k.take_character(binding()).unwrap_err(),
        CharacterRegistrationError::DurabilityPending
    );
    let mut forged = proposal.clone();
    if let NpcEffect::Property { change, .. } = &mut forged.effect {
        change.after = Some(PropertyValue::Int(77));
    }
    assert_eq!(k.confirm_npc_committed(&forged), Err(NpcFailure::Conflict));
    k.confirm_npc_committed(&proposal).unwrap();
    let mut speech = Vec::new();
    for _ in 0..20 {
        assert!(k.step().unwrap().is_empty());
        while let Some(p) = k.take_npc_proposal() {
            k.confirm_npc_committed(&p).unwrap();
        }
        while let Some(n) = k.take_npc_notification() {
            speech.push(n);
        }
    }
    assert_eq!(
        k.world()
            .properties(EntityId(1))
            .unwrap()
            .get(PropertyFamily::Int, 999),
        Some(&PropertyValue::Int(42))
    );
    assert_eq!(k.character(EntityId(1)).unwrap().available_experience(), 50);
    assert_eq!(
        k.character(EntityId(1))
            .unwrap()
            .luminance()
            .unwrap()
            .available,
        1000
    );
    assert_eq!(
        k.npc_quest(EntityId(1), "FLAG@comment")
            .unwrap()
            .completions,
        1
    );
    assert!(speech.iter().any(|n|matches!(&n.operation,bace_gameplay_api::NpcOperation::Text{text,..} if text=="Keeper greets Player")));
}
#[test]
fn full_notification_capacity_retains_next_native_action_while_physics_ticks() {
    // The player and native source each own a quest registry. Admit both, then
    // fill the same bounded notification capacity before testing backpressure.
    let mut k = kernel(2);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![
                EmoteAction {
                    r#type: 8,
                    message: Some("one".into()),
                    ..Default::default()
                },
                EmoteAction {
                    r#type: 8,
                    message: Some("two".into()),
                    ..Default::default()
                },
                EmoteAction {
                    r#type: 8,
                    message: Some("three".into()),
                    ..Default::default()
                },
            ],
        )],
    );
    k.step().unwrap();
    for _ in 0..10 {
        k.step().unwrap();
    }
    assert_eq!(k.ticks(), 11);
    assert!(
        matches!(k.take_npc_notification().unwrap().operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="one")
    );
    assert!(
        matches!(k.take_npc_notification().unwrap().operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="two")
    );
    assert!(
        k.take_npc_notification().is_none(),
        "only the two admitted slots may be occupied"
    );
    k.step().unwrap();
    assert!(
        matches!(k.take_npc_notification().unwrap().operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="three")
    );
    assert!(k.take_npc_notification().is_none());
}
#[test]
fn native_title_and_contract_queries_read_receipt_adopted_character_owners() {
    let mut k = kernel(8);
    k.register_npc_character_services(
        EntityId(1),
        bace_character::CharacterServiceState {
            level: 1,
            total_experience: 0,
            total_skill_credits: None,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
        },
        bace_quests::ContractRegistry::restore(vec![]).unwrap(),
    )
    .unwrap();
    k.register_npc_contract_definitions(vec![7]).unwrap();
    install(
        &mut k,
        vec![
            set(
                7,
                None,
                vec![
                    EmoteAction {
                        r#type: 34,
                        amount: Some(33),
                        ..Default::default()
                    },
                    EmoteAction {
                        r#type: 119,
                        stat: Some(7),
                        ..Default::default()
                    },
                    EmoteAction {
                        r#type: 71,
                        min: Some(1),
                        max: Some(1),
                        message: Some("titles".into()),
                        ..Default::default()
                    },
                ],
            ),
            set(
                35,
                Some("titles"),
                vec![EmoteAction {
                    r#type: 8,
                    message: Some("title present".into()),
                    ..Default::default()
                }],
            ),
        ],
    );
    k.step().unwrap();
    let title = k.take_npc_proposal().unwrap();
    assert!(matches!(title.effect, NpcEffect::CharacterService { .. }));
    assert!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .titles
            .is_empty()
    );
    k.confirm_npc_committed(&title).unwrap();
    assert_eq!(k.npc_character_services(EntityId(1)).unwrap().titles, [33]);
    k.step().unwrap();
    let contract = k.take_npc_proposal().unwrap();
    assert!(matches!(contract.effect, NpcEffect::Contract { .. }));
    assert!(k.npc_contracts(EntityId(1)).unwrap().entries().is_empty());
    k.confirm_npc_committed(&contract).unwrap();
    assert_eq!(k.npc_contracts(EntityId(1)).unwrap().entries()[0].id, 7);
    for _ in 0..4 {
        k.step().unwrap();
    }
    assert!(
        matches!(k.take_npc_notification().unwrap().operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="title present")
    );
}

#[test]
fn durable_commit_marker_resumes_vm_without_reapplying_actor_effect() {
    let mut k = kernel(8);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 53,
                stat: Some(999),
                amount: Some(42),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let logical = k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now;
    let checkpoint = k
        .preview_npc_committed_checkpoint(
            &proposal,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
            logical,
        )
        .unwrap();
    assert!(checkpoint.pending[0].adopted);
    assert_eq!(
        k.world()
            .properties(EntityId(1))
            .unwrap()
            .get(PropertyFamily::Int, 999),
        None
    );
    k.confirm_npc_committed(&proposal).unwrap();
    let aggregate = k.character(EntityId(1)).unwrap().revision();
    let local = k.world().properties(EntityId(1)).unwrap().revision();
    // Models loading the committed actor aggregate and its same-transaction marker.
    k.restore_npc_source(checkpoint).unwrap();
    assert!(k.take_npc_proposal().is_none());
    assert!(k.step().unwrap().is_empty());
    assert!(
        !k.checkpoint_npc_source(EntityId(2))
            .unwrap()
            .pending
            .is_empty()
    );
    k.acknowledge_npc_recovery_ready(EntityId(2)).unwrap();
    assert!(k.step().unwrap().is_empty());
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), aggregate);
    assert_eq!(k.world().properties(EntityId(1)).unwrap().revision(), local);
    assert!(
        k.checkpoint_npc_source(EntityId(2))
            .unwrap()
            .pending
            .is_empty()
    );
    assert!(k.confirm_npc_committed(&proposal).is_err());
}

fn xp_kernel() -> Kernel {
    xp_kernel_with_rates(1.0, 1.0)
}
fn xp_kernel_with_rates(global: f64, quest: f64) -> Kernel {
    xp_kernel_with_credit(global, quest, None, 0)
}
fn xp_kernel_with_credit(global: f64, quest: f64, total: Option<i32>, credits: u32) -> Kernel {
    let mut k = kernel_with_training(16, true);
    k.register_npc_character_services(
        EntityId(1),
        bace_character::CharacterServiceState {
            level: 1,
            total_experience: 0,
            total_skill_credits: total,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
        },
        bace_quests::ContractRegistry::restore(vec![]).unwrap(),
    )
    .unwrap();
    k.register_npc_experience_table(
        Arc::new(
            bace_character::CharacterLevelTable::prepare(
                vec![0, 0, 100, 300],
                vec![0, 0, credits, 0],
            )
            .unwrap(),
        ),
        global,
        quest,
    )
    .unwrap();
    k.register_inventory_container(bace_inventory::InventoryContainer {
        id: EntityId(1),
        revision: 0,
        root_owner: Some(EntityId(1)),
        slots: 8,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 0,
    })
    .unwrap();
    k.register_magic_registry(
        EntityId(1),
        bace_magic::EnchantmentRegistry::new(16).unwrap(),
        true,
    )
    .unwrap();
    k
}
fn queued_xp_program(intervene: bool) -> Arc<NativeProgram> {
    let mut actions = vec![EmoteAction {
        r#type: 62,
        amount: Some(150),
        ..Default::default()
    }];
    if intervene {
        actions.push(EmoteAction {
            r#type: 115,
            stat: Some(1),
            amount64: Some(80),
            ..Default::default()
        });
    }
    actions.push(if intervene {
        EmoteAction {
            r#type: 114,
            stat: Some(1),
            min64: Some(80),
            max64: Some(80),
            message: Some("before".into()),
            ..Default::default()
        }
    } else {
        EmoteAction {
            r#type: 36,
            stat: Some(25),
            min: Some(1),
            max: Some(1),
            message: Some("before".into()),
            ..Default::default()
        }
    });
    Arc::new(
        NativeProgram::prepare(
            vec![
                set(7, None, actions),
                set(
                    22,
                    Some("before"),
                    vec![EmoteAction {
                        r#type: 8,
                        message: Some("before recipient update".into()),
                        ..Default::default()
                    }],
                ),
                set(
                    23,
                    Some("before"),
                    vec![EmoteAction {
                        r#type: 8,
                        message: Some("WRONG early update".into()),
                        ..Default::default()
                    }],
                ),
            ],
            NativeLimits::default(),
        )
        .unwrap(),
    )
}
fn start_xp(k: &mut Kernel, p: Arc<NativeProgram>) {
    k.register_native_npc(EntityId(2), p, 3.0).unwrap();
    k.start_npc_emote(
        EntityId(2),
        Some(EntityId(1)),
        NativeTrigger {
            category: 7,
            ..Default::default()
        },
        [19; 16],
        30,
        true,
    )
    .unwrap();
}
#[test]
fn queued_xp_outer_query_runs_first_and_recipient_prepares_current_state() {
    for intervene in [false, true] {
        let mut k = xp_kernel();
        start_xp(&mut k, queued_xp_program(intervene));
        assert!(k.step().unwrap().is_empty());
        let admission = k.take_npc_proposal().unwrap();
        assert!(matches!(
            admission.effect,
            NpcEffect::QueuedExperience {
                amount: 150,
                phase: bace_simulation::NpcQueuedExperiencePhase::AwaitingAdmission,
                ..
            }
        ));
        assert_eq!(k.npc_character_services(EntityId(1)).unwrap().level, 1);
        assert!(k.confirm_npc_committed(&admission).is_err());
        let now = k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now;
        let preview = k
            .preview_npc_queued_experience_admission(&admission, now)
            .unwrap();
        assert!(preview.pending[0].detached);
        assert!(!preview.pending[0].adopted);
        assert_eq!(preview.vm.detached, vec![admission.ticket]);
        k.admit_npc_queued_experience(&admission).unwrap();
        assert!(k.admit_npc_queued_experience(&admission).is_err());
        assert!(k.step().unwrap().is_empty());
        if intervene {
            let synchronous = k.take_npc_proposal().unwrap();
            assert!(matches!(
                synchronous.effect,
                NpcEffect::CharacterService { .. }
            ));
            assert!(k.take_npc_proposal().is_none());
            k.confirm_npc_committed(&synchronous).unwrap();
            assert!(k.step().unwrap().is_empty());
        }
        let message = k.take_npc_notification().unwrap();
        assert!(
            matches!(message.operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="before recipient update")
        );
        let observed = k.npc_character_services(EntityId(1)).unwrap();
        assert_eq!(
            (
                observed.level,
                observed.total_experience,
                k.character(EntityId(1)).unwrap().available_experience()
            ),
            queued_xp_oracle(if intervene { "intervening" } else { "plain" }, "query")
        );
        let recipient = k.take_npc_proposal().unwrap();
        let NpcEffect::EarnedExperience { change, .. } = &recipient.effect else {
            panic!("recipient stage")
        };
        assert_eq!(
            change.services.before.total_experience,
            if intervene { 80 } else { 0 }
        );
        assert_eq!(change.services.after.level, 2);
        assert_eq!(k.npc_character_services(EntityId(1)).unwrap().level, 1);
        k.confirm_npc_committed(&recipient).unwrap();
        assert_eq!(
            k.npc_character_services(EntityId(1))
                .unwrap()
                .total_experience,
            if intervene { 230 } else { 150 }
        );
        assert_eq!(k.npc_character_services(EntityId(1)).unwrap().level, 2);
        assert_eq!(
            k.character(EntityId(1)).unwrap().available_experience(),
            250
        );
        let observed = k.npc_character_services(EntityId(1)).unwrap();
        assert_eq!(
            (
                observed.level,
                observed.total_experience,
                k.character(EntityId(1)).unwrap().available_experience()
            ),
            queued_xp_oracle(if intervene { "intervening" } else { "plain" }, "recipient")
        );
        assert!(k.confirm_npc_committed(&recipient).is_err());
    }
}
#[test]
fn admitted_queued_xp_recovers_without_early_query_or_duplicate_grant() {
    let program = queued_xp_program(false);
    let mut original = xp_kernel();
    start_xp(&mut original, program.clone());
    original.step().unwrap();
    let admission = original.take_npc_proposal().unwrap();
    let now = original
        .checkpoint_npc_source(EntityId(2))
        .unwrap()
        .logical_now;
    let ready = original
        .preview_npc_queued_experience_admission(&admission, now)
        .unwrap();
    let mut recovered = xp_kernel();
    recovered
        .register_native_npc(EntityId(2), program, 3.0)
        .unwrap();
    recovered.restore_npc_source(ready).unwrap();
    assert!(recovered.step().unwrap().is_empty());
    assert!(recovered.take_npc_notification().is_none());
    assert_eq!(
        recovered
            .npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        0
    );
    recovered
        .acknowledge_npc_recovery_ready(EntityId(2))
        .unwrap();
    assert!(recovered.step().unwrap().is_empty());
    let recipient = recovered.take_npc_proposal().unwrap();
    assert!(matches!(
        recipient.effect,
        NpcEffect::EarnedExperience { .. }
    ));
    assert!(
        matches!(recovered.take_npc_notification().unwrap().operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="before recipient update")
    );
    recovered.confirm_npc_committed(&recipient).unwrap();
    assert_eq!(
        recovered
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        250
    );
    assert!(recovered.step().unwrap().is_empty());
    assert!(recovered.take_npc_proposal().is_none());
}
#[test]
fn ordinary_shared_xp_is_explicitly_gated_without_mutation() {
    let mut k = xp_kernel();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 2,
                amount: Some(150),
                ..Default::default()
            }],
        )],
    );
    assert!(!k.step().unwrap().is_empty());
    assert!(k.take_npc_proposal().is_none());
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_experience(),
        100
    );
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        0
    );
}

fn queued_xp_oracle(case: &str, stage: &str) -> (u32, u64, u64) {
    let row = include_str!("../../../gameplay/bace-emotes/tests/fixtures/queued_xp.csv")
        .lines()
        .find(|row| row.starts_with(&format!("{case},{stage},")))
        .unwrap();
    let fields: Vec<_> = row.split(',').collect();
    (
        fields[2].parse().unwrap(),
        fields[3].parse().unwrap(),
        fields[4].parse().unwrap(),
    )
}
#[test]
fn queued_xp_recipient_commit_marker_never_replays_xp_after_restore() {
    let mut k = xp_kernel();
    start_xp(&mut k, queued_xp_program(false));
    k.step().unwrap();
    let admission = k.take_npc_proposal().unwrap();
    k.admit_npc_queued_experience(&admission).unwrap();
    k.step().unwrap();
    let recipient = k.take_npc_proposal().unwrap();
    let now = k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now;
    let marker = k
        .preview_npc_committed_checkpoint(
            &recipient,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
            now,
        )
        .unwrap();
    assert!(marker.pending[0].adopted);
    assert!(marker.pending[0].detached);
    k.confirm_npc_committed(&recipient).unwrap();
    k.restore_npc_source(marker).unwrap();
    k.acknowledge_npc_recovery_ready(EntityId(2)).unwrap();
    assert!(k.step().unwrap().is_empty());
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_experience(),
        250
    );
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        150
    );
    assert!(k.take_npc_proposal().is_none());
}

#[test]
fn queued_xp_captures_source_float_product_before_double_rate_and_even_rounding() {
    for row in include_str!("../../../gameplay/bace-emotes/tests/fixtures/queued_xp.csv")
        .lines()
        .filter(|line| line.starts_with("scaled,"))
    {
        let fields: Vec<_> = row.split(',').collect();
        let amount: i32 = fields[1].parse().unwrap();
        let global: f64 = fields[2].parse().unwrap();
        let expected: u64 = fields[3].parse().unwrap();
        let mut k = xp_kernel_with_rates(global, 1.0);
        install(
            &mut k,
            vec![set(
                7,
                None,
                vec![EmoteAction {
                    r#type: 62,
                    amount: Some(amount),
                    ..Default::default()
                }],
            )],
        );
        assert!(k.step().unwrap().is_empty());
        let proposal = k.take_npc_proposal().unwrap();
        assert!(
            matches!(proposal.effect, NpcEffect::QueuedExperience{amount,..} if amount == expected),
            "{row}"
        );
        assert_eq!(
            k.npc_character_services(EntityId(1))
                .unwrap()
                .total_experience,
            0
        );
    }
}

#[test]
fn queued_xp_retains_logout_participant_before_and_after_admission() {
    for admitted in [false, true] {
        let mut k = xp_kernel();
        register_logout_presence(&mut k);
        start_xp(&mut k, queued_xp_program(false));
        k.step().unwrap();
        let admission = k.take_npc_proposal().unwrap();
        if admitted {
            k.admit_npc_queued_experience(&admission).unwrap();
        }
        let revision = k.character(EntityId(1)).unwrap().revision();
        assert!(matches!(
            k.take_player_state(binding()),
            Err(CharacterRegistrationError::DurabilityPending)
        ));
        assert_eq!(k.character(EntityId(1)).unwrap().revision(), revision);
        if !admitted {
            k.admit_npc_queued_experience(&admission).unwrap();
        }
        assert!(k.step().unwrap().is_empty());
        let recipient = k.take_npc_proposal().unwrap();
        k.confirm_npc_committed(&recipient).unwrap();
        let state = k.take_player_state(binding()).unwrap();
        assert_eq!(
            state.character.native_services.unwrap().total_experience,
            150
        );
        assert_eq!(state.character.progression.available_experience(), 250);
    }
}

#[test]
fn queued_level_reward_adopts_both_credit_owners_and_preserves_nullable_total() {
    for total in [Some(10), None, Some(-2)] {
        let mut k = xp_kernel_with_credit(1.0, 1.0, total, 1);
        register_logout_presence(&mut k);
        start_xp(&mut k, queued_xp_program(false));
        k.step().unwrap();
        let admission = k.take_npc_proposal().unwrap();
        k.admit_npc_queued_experience(&admission).unwrap();
        k.step().unwrap();
        let recipient = k.take_npc_proposal().unwrap();
        let NpcEffect::EarnedExperience { change, .. } = &recipient.effect else {
            panic!("recipient")
        };
        assert_eq!(change.services.before.total_skill_credits, total);
        assert_eq!(
            change.services.after.total_skill_credits,
            total.map(|v| v + 1)
        );
        assert_eq!(
            k.character(EntityId(1)).unwrap().available_skill_credits(),
            Some(5)
        );
        assert_eq!(
            k.npc_character_services(EntityId(1))
                .unwrap()
                .total_skill_credits,
            total
        );
        k.confirm_npc_committed(&recipient).unwrap();
        assert_eq!(
            k.character(EntityId(1)).unwrap().available_skill_credits(),
            Some(6)
        );
        assert_eq!(
            k.npc_character_services(EntityId(1))
                .unwrap()
                .total_skill_credits,
            total.map(|v| v + 1)
        );
        let complete = k.take_player_state(binding()).unwrap();
        assert_eq!(
            complete
                .character
                .native_services
                .unwrap()
                .total_skill_credits,
            total.map(|v| v + 1)
        );
        assert_eq!(
            complete.character.progression.available_skill_credits(),
            Some(6)
        );
    }
}

#[test]
fn queued_quest_shared_owner_requires_joint_receipt_and_does_not_regrant() {
    let mut k = xp_kernel();
    k.configure_social_experience(Arc::new(
        bace_character::CharacterLevelTable::prepare(vec![0, 0, 100, 300], vec![0, 0, 0, 0])
            .unwrap(),
    ))
    .unwrap();
    start_xp(&mut k, queued_xp_program(false));
    assert!(k.step().unwrap().is_empty());
    let admission = k.take_npc_proposal().unwrap();
    k.admit_npc_queued_experience(&admission).unwrap();
    assert!(k.step().unwrap().is_empty());
    let ticket = k.take_allegiance_proposal().expect("joint reward stage");
    let npc = ticket.npc.as_ref().expect("source workflow binding");
    assert!(k.take_npc_proposal().is_none());
    assert!(k.confirm_npc_committed(npc).is_err());
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        0
    );
    let now = k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now;
    let preview = k
        .preview_npc_committed_checkpoint(
            npc,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
            now,
        )
        .unwrap();
    assert!(
        preview
            .pending
            .iter()
            .any(|p| p.proposal == *npc && p.adopted && p.detached)
    );
    k.confirm_allegiance_committed(&ticket).unwrap();
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        150
    );
    assert!(k.step().unwrap().is_empty());
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        150
    );
    assert!(
        k.checkpoint_npc_source(EntityId(2))
            .unwrap()
            .pending
            .is_empty()
    );
    assert!(k.confirm_allegiance_committed(&ticket).is_err());
}

// Complete player transfer now includes the admitted social identity. Tests
// that exercise logout must register that owner rather than bypass its fence.
fn register_logout_presence(k: &mut Kernel) {
    k.register_social_presence(
        bace_social::SocialPresence {
            identity: bace_gameplay_api::social::SocialIdentity {
                character: binding().actor,
                account: binding().account,
                name: "Player".into(),
            },
            access: 0,
            online: true,
            appear_offline: false,
            afk: false,
            gagged: false,
            olthoi: false,
            no_olthoi_talk: false,
            ignore_fellowship_requests: false,
            auto_accept_fellowship: false,
            share_fellowship_loot: false,
            society: 0,
            listen_allegiance: true,
            listen_general: true,
            listen_trade: true,
            listen_lfg: true,
            listen_roleplay: true,
            listen_society: true,
        },
        Default::default(),
    )
    .unwrap();
}
