use bace_gameplay_api::{CharacterBinding, CharacterUi, SessionId, visibility::*};
use bace_geometry::{Aabb, Vec3};
use bace_runtime::simulation::{SimulationConfig, SimulationWorker};
use bace_simulation::{Command, Kernel, OwnedUiState};
use bace_types::{AccountId, CellId, EntityId};
use std::sync::Arc;
fn binding(id: u32) -> CharacterBinding {
    CharacterBinding {
        session: SessionId(u64::from(id)),
        account: AccountId(u64::from(id)),
        actor: EntityId(id),
    }
}
fn kernel() -> Kernel {
    let mut world = bace_world::World::default();
    let cell = CellId(0x10100001);
    world
        .register_scene(
            cell,
            bace_physics::SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = bace_physics::Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32, 0., 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
        )
        .unwrap();
        world
            .insert(bace_entity::Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                bace_entity::Combatant::new(bace_entity::CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 1,
                    melee_range: 1.,
                    attack_duration: 1.,
                    strike_offsets: vec![0.5],
                    player: true,
                })
                .unwrap(),
            )
            .unwrap();
    }
    let mut kernel = Kernel::with_gameplay_limits(world, 16, 4, 16).unwrap();
    for id in 1..=2 {
        let rank = bace_character::RankTable::new(&[0, 10, 100]).unwrap();
        let character = bace_character::CharacterProgression::new(
            &[],
            Arc::new(bace_character::ProgressionTables {
                attributes: rank.clone(),
                vitals: rank.clone(),
                trained_skills: rank.clone(),
                specialized_skills: rank,
            }),
            0,
            0,
        )
        .unwrap();
        kernel.register_character(binding(id), character).unwrap();
        kernel
            .register_character_ui(
                binding(id),
                OwnedUiState {
                    state: CharacterUi::default(),
                    known_spells: vec![],
                    component_templates: vec![],
                    entered: id == 1,
                },
            )
            .unwrap();
    }
    kernel
}
#[test]
fn loading_player_is_invisible_until_exact_entered_fence() {
    let mut kernel = kernel();
    assert!(
        kernel
            .visibility_snapshot(binding(1), 16)
            .unwrap()
            .candidates
            .is_empty()
    );
    let mut wrong = binding(2);
    wrong.session = SessionId(30);
    assert!(kernel.character_entered(wrong).is_err());
    assert!(
        kernel
            .visibility_snapshot(binding(1), 16)
            .unwrap()
            .candidates
            .is_empty()
    );
    kernel.character_entered(binding(2)).unwrap();
    let result = kernel.visibility_snapshot(binding(1), 16).unwrap();
    assert_eq!(result.candidates[0].entity, EntityId(2));
    assert_eq!(
        kernel
            .visibility_snapshot(binding(2), 16)
            .unwrap()
            .candidates[0]
            .entity,
        EntityId(1)
    );
}
#[test]
fn real_worker_pressure_and_recovery_preserve_every_query_correlation() {
    let mut kernel = kernel();
    kernel.character_entered(binding(2)).unwrap();
    for id in 1..=4 {
        kernel
            .enqueue(Command::Visibility(Box::new(VisibilityRequest {
                correlation: id,
                binding: binding(1),
                limit: 16,
                candidates: Vec::with_capacity(16),
            })))
            .unwrap();
        kernel
            .enqueue(Command::ObjectView(Box::new(ObjectViewRequest {
                correlation: id,
                binding: binding(1),
                entities: vec![EntityId(2)],
            })))
            .unwrap();
    }
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 1,
            tick_limit: Some(8),
            real_time: false,
        },
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert_eq!(exit.report.ticks, 8);
    assert!(exit.unprocessed_commands.is_empty());
    let mut pvs = std::collections::BTreeSet::new();
    let mut views = std::collections::BTreeSet::new();
    for output in exit.visibility.outcomes.drain(..) {
        assert!(output.result.is_ok());
        assert!(pvs.insert(output.correlation));
    }
    for output in exit.object_view.outcomes.drain(..) {
        assert!(output.result.is_ok());
        assert!(views.insert(output.correlation));
    }
    for _ in 0..12 {
        while let Some(output) = exit.kernel.take_visibility_outcome() {
            assert!(output.result.is_ok());
            assert!(pvs.insert(output.correlation));
        }
        while let Some(output) = exit.kernel.take_object_view_outcome() {
            assert!(output.result.is_ok());
            assert!(views.insert(output.correlation));
        }
        exit.kernel.step().unwrap();
    }
    assert_eq!(pvs, (1..=4).collect());
    assert_eq!(views, (1..=4).collect());
}
