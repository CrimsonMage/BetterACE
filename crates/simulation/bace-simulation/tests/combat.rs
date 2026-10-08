use std::sync::Arc;

use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::Actor;
use bace_entity::{Combatant, CombatantProfile};
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionTarget, SessionId, SkillAdvancement,
};
use bace_gameplay_api::{CombatChange, CombatRejection, CombatRequest};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::CombatEvent;
use bace_simulation::{Command, Kernel};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;

fn kernel(commands: usize, characters: usize, outcomes: usize) -> Kernel {
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
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
    }
    for id in 1..=2 {
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 20,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: id == 1,
                })
                .unwrap(),
            )
            .unwrap();
    }
    let mut kernel = Kernel::with_gameplay_limits(world, commands, characters, outcomes).unwrap();
    kernel.register_character(binding(), character()).unwrap();
    kernel
}

fn character() -> CharacterProgression {
    let table = RankTable::new(&[0, 1, 10, 100]).unwrap();
    CharacterProgression::new(
        &[TraitProgress {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            experience_spent: 0,
            advancement: SkillAdvancement::Inactive,
        }],
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
}

fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(9),
        account: AccountId(4),
        actor: EntityId(1),
    }
}

fn context(sequence: u32) -> ActionContext {
    let binding = binding();
    ActionContext {
        session: binding.session,
        account: binding.account,
        actor: binding.actor,
        sequence,
    }
}

fn combat(sequence: u32, request: CombatRequest) -> Command {
    Command::Combat {
        context: context(sequence),
        request,
    }
}
fn attack(sequence: u32) -> Command {
    combat(
        sequence,
        CombatRequest::TargetedMelee {
            target: EntityId(2),
            height: 2,
            power: 0.5,
        },
    )
}
#[test]
fn authenticated_melee_runs_hooks_then_changes_world_health_once() {
    let mut kernel = kernel(8, 2, 8);
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel.enqueue(attack(2)).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.take_combat_outcome().unwrap().result,
        Ok(CombatChange::Mode(2))
    );
    assert_eq!(
        kernel.take_combat_outcome().unwrap().result,
        Ok(CombatChange::AttackStarted {
            target: EntityId(2)
        })
    );
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().health(), 10);
    kernel.step().unwrap();
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().health(), 0);
    assert!(matches!(
        kernel.take_combat_event(),
        Some(CombatEvent::Damage {
            attacker: Some(EntityId(1)),
            death_blow: None,
            target: EntityId(2),
            amount: 10,
            current: 0,
            maximum: 10,
            killed: true,
            revision: 1,
            ..
        })
    ));
    for _ in 0..10 {
        kernel.step().unwrap();
    }
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().revision(), 1);
}
#[test]
fn output_pressure_does_not_consume_queued_attack_or_sequence() {
    let mut kernel = kernel(8, 2, 1);
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel.enqueue(attack(2)).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.queued_commands(), 1);
    for _ in 0..5 {
        kernel.step().unwrap();
    }
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().health(), 10);
    kernel.take_combat_outcome().unwrap();
    kernel.step().unwrap();
    assert!(kernel.take_combat_outcome().unwrap().result.is_ok());
    kernel.step().unwrap();
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().health(), 0);
}
#[test]
fn invalid_power_and_replay_do_not_mutate_health() {
    let mut kernel = kernel(8, 2, 8);
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel
        .enqueue(combat(
            2,
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: f32::NAN,
            },
        ))
        .unwrap();
    kernel.enqueue(attack(2)).unwrap();
    kernel.step().unwrap();
    kernel.take_combat_outcome().unwrap();
    assert_eq!(
        kernel.take_combat_outcome().unwrap().result,
        Err(CombatRejection::InvalidRequest)
    );
    assert_eq!(
        kernel.take_combat_outcome().unwrap().result,
        Err(CombatRejection::StaleSequence)
    );
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().health(), 10);
}
#[test]
fn target_teleport_before_impact_cancels_damage_and_old_logout_attack_is_cancelled() {
    let mut kernel = kernel(8, 2, 8);
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel.enqueue(attack(2)).unwrap();
    kernel.step().unwrap();
    kernel
        .enqueue(Command::ServerTeleport {
            actor: EntityId(2),
            cell: CellId(1),
            position: Vec3::new(8.0, 0.0, 0.5),
        })
        .unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.world().combatant(EntityId(2)).unwrap().health(), 10);
    assert!(matches!(
        kernel.take_combat_event(),
        Some(CombatEvent::Finished {
            actor: EntityId(1),
            cancelled: true,
            ..
        })
    ));
    kernel.take_character(binding()).unwrap();
    kernel.enqueue(attack(3)).unwrap();
    kernel.step().unwrap();
    while let Some(outcome) = kernel.take_combat_outcome() {
        if outcome.context.sequence == 3 {
            assert_eq!(outcome.result, Err(CombatRejection::NotBound));
        }
    }
}
