//! Bounded adapter delivery must retain pending valuable operations and UI state.
use bace_character::*;
use bace_gameplay_api::*;
use bace_runtime::simulation::{SimulationConfig, SimulationWorker};
use bace_simulation::*;
use bace_types::{AccountId, EntityId};
use std::{sync::Arc, time::Duration};
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
fn kernel() -> Kernel {
    let ranks = RankTable::new(&[0, 10, 50]).unwrap();
    let progression = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(31),
                advancement: SkillAdvancement::Untrained,
                experience_spent: 0,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        1000,
        1,
    )
    .unwrap()
    .with_training(
        Arc::new(
            SkillTrainingRules::new(&[SkillCosts {
                skill: 31,
                trained_cost: 4,
                specialized_cost: 6,
            }])
            .unwrap(),
        ),
        20,
        &[],
    )
    .unwrap();
    let mut kernel = synthetic_scenario(1, 0).unwrap();
    let binding = CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
    };
    kernel.register_character(binding, progression).unwrap();
    kernel
        .register_character_ui(
            binding,
            OwnedUiState {
                state: Default::default(),
                known_spells: vec![100],
                component_templates: vec![200],
                entered: true,
            },
        )
        .unwrap();
    kernel
}
#[test]
fn skill_worker_never_applies_before_receipt_and_shutdown_retains_pending_owner() {
    let mut kernel = kernel();
    kernel
        .enqueue(Command::TrainSkill {
            context: context(1),
            request: TrainSkill {
                skill: 31,
                quoted_credits: 4,
            },
        })
        .unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 2,
            tick_limit: Some(2),
            real_time: false,
        },
    )
    .unwrap();
    let proposed = worker
        .skill_outcomes()
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    let SkillStage::Proposed(ticket) = proposed.result.unwrap() else {
        panic!("proposal")
    };
    let mut exit = worker.wait_recover().unwrap();
    assert_eq!(
        exit.kernel
            .character(EntityId(1))
            .unwrap()
            .available_skill_credits(),
        Some(20)
    );
    assert!(
        exit.kernel
            .take_player_state(CharacterBinding {
                session: SessionId(1),
                account: AccountId(1),
                actor: EntityId(1)
            })
            .is_err()
    );
    exit.kernel
        .enqueue(Command::CommitSkill { ticket })
        .unwrap();
    exit.kernel.step().unwrap();
    assert!(
        matches!(exit.kernel.take_skill_outcome().unwrap().result, Ok(SkillStage::Committed(t)) if t == ticket)
    );
    assert_eq!(
        exit.kernel
            .character(EntityId(1))
            .unwrap()
            .available_skill_credits(),
        Some(16)
    );
    assert!(exit.kernel.confirm_skill_committed(ticket).is_err());
}
#[test]
fn ui_and_crafting_replies_survive_full_channels_without_stopping_ticks() {
    let mut kernel = kernel();
    for sequence in 1..=6 {
        kernel
            .enqueue(Command::Ui {
                context: context(sequence),
                request: UiRequest::Filters(sequence),
            })
            .unwrap();
        kernel
            .enqueue(Command::Crafting(CraftingCommand {
                correlation: u64::from(sequence),
                action: CraftingCommandKind::Retry { operation: 999 },
            }))
            .unwrap();
    }
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 2,
            tick_limit: Some(3),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert_eq!(exit.report.ticks, 3);
    let mut ui = exit.undelivered_ui_outcomes;
    while let Some(outcome) = exit.kernel.take_ui_outcome() {
        ui.push(outcome);
    }
    let mut crafting = exit.undelivered_crafting_outcomes;
    while let Some(outcome) = exit.kernel.take_crafting_outcome() {
        crafting.push(outcome);
    }
    assert_eq!(ui.len(), 6);
    assert_eq!(crafting.len(), 6);
    assert!(ui.iter().all(|r| r.result.is_ok()));
    assert!(crafting.iter().all(|r| r.result.is_err()));
    assert_eq!(exit.kernel.character_ui(EntityId(1)).unwrap().filters, 6);
    assert_eq!(
        crafting.iter().map(|r| r.correlation).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
}
