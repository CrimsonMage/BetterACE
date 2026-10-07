use std::sync::Arc;

use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::Actor;
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionActionRejection, ProgressionRejection,
    ProgressionTarget, RaiseProgression, SessionId, SkillAdvancement,
};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::{CharacterRegistrationError, Command, Kernel, SimulationError};
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
    Kernel::with_gameplay_limits(world, commands, characters, outcomes).unwrap()
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

fn command(context: ActionContext, amount: u32) -> Command {
    Command::RaiseProgression {
        context,
        request: RaiseProgression {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            amount,
        },
    }
}

#[test]
fn actor_account_and_session_bindings_are_unique_and_failed_registration_returns_state() {
    let mut kernel = kernel(8, 2, 8);
    let missing = CharacterBinding {
        actor: EntityId(99),
        ..binding()
    };
    let (reason, retained) = kernel.register_character(missing, character()).unwrap_err();
    assert_eq!(reason, CharacterRegistrationError::MissingActor);
    assert_eq!(retained.available_experience(), 100);
    kernel.register_character(binding(), retained).unwrap();
    for (candidate, expected) in [
        (binding(), CharacterRegistrationError::ActorAlreadyBound),
        (
            CharacterBinding {
                actor: EntityId(2),
                session: SessionId(10),
                ..binding()
            },
            CharacterRegistrationError::AccountAlreadyBound,
        ),
        (
            CharacterBinding {
                actor: EntityId(2),
                account: AccountId(5),
                ..binding()
            },
            CharacterRegistrationError::SessionAlreadyBound,
        ),
    ] {
        let (error, retained) = kernel
            .register_character(candidate, character())
            .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(retained.available_experience(), 100);
    }
    assert_eq!(kernel.world().states().count(), 2);
}

#[test]
fn character_capacity_cannot_be_bypassed_by_new_identities() {
    let mut kernel = kernel(8, 1, 8);
    kernel.register_character(binding(), character()).unwrap();
    let other = CharacterBinding {
        actor: EntityId(2),
        account: AccountId(5),
        session: SessionId(10),
    };
    assert_eq!(
        kernel.register_character(other, character()).unwrap_err().0,
        CharacterRegistrationError::Capacity
    );
}

#[test]
fn mismatched_identity_never_consumes_sequence_or_changes_state() {
    let mut kernel = kernel(8, 1, 8);
    kernel.register_character(binding(), character()).unwrap();
    for forged in [
        ActionContext {
            account: AccountId(100),
            ..context(100)
        },
        ActionContext {
            session: SessionId(100),
            ..context(100)
        },
    ] {
        kernel.enqueue(command(forged, 10)).unwrap();
        kernel.step().unwrap();
        let result = kernel.take_progression_outcome().unwrap();
        assert_eq!(result.context, forged);
        assert_eq!(
            result.result,
            Err(ProgressionActionRejection::OwnershipMismatch)
        );
        assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 0);
    }
    kernel.enqueue(command(context(0), 10)).unwrap();
    kernel.step().unwrap();
    let accepted = kernel.take_progression_outcome().unwrap();
    assert_eq!(accepted.context, context(0));
    assert_eq!(accepted.result.unwrap().available_experience, 90);
    assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 1);
}

#[test]
fn serial_sequences_accept_wrap_but_reject_duplicates_old_and_half_range_values() {
    let mut kernel = kernel(8, 1, 8);
    kernel.register_character(binding(), character()).unwrap();
    for sequence in [u32::MAX - 1, u32::MAX, 0, 1] {
        kernel.enqueue(command(context(sequence), 1)).unwrap();
        kernel.step().unwrap();
        assert!(kernel.take_progression_outcome().unwrap().result.is_ok());
    }
    for sequence in [1, u32::MAX, 0x8000_0001] {
        kernel.enqueue(command(context(sequence), 1)).unwrap();
        kernel.step().unwrap();
        assert_eq!(
            kernel.take_progression_outcome().unwrap().result,
            Err(ProgressionActionRejection::StaleSequence)
        );
        assert_eq!(
            kernel
                .character(EntityId(1))
                .unwrap()
                .available_experience(),
            96
        );
        assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 4);
    }
}

#[test]
fn domain_rejection_is_correlated_and_cannot_be_replayed_as_a_different_request() {
    let mut kernel = kernel(8, 1, 8);
    kernel.register_character(binding(), character()).unwrap();
    kernel.enqueue(command(context(5), 101)).unwrap();
    kernel.enqueue(command(context(5), 1)).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.take_progression_outcome().unwrap().result,
        Err(ProgressionActionRejection::Domain(
            ProgressionRejection::InsufficientExperience
        ))
    );
    assert_eq!(
        kernel.take_progression_outcome().unwrap().result,
        Err(ProgressionActionRejection::StaleSequence)
    );
    assert_eq!(
        kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        100
    );
    assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 0);
    kernel.enqueue(command(context(6), 1)).unwrap();
    kernel.step().unwrap();
    assert!(kernel.take_progression_outcome().unwrap().result.is_ok());
}

#[test]
fn full_outbox_retains_fifo_request_and_continues_ticks_without_partial_mutation() {
    let mut kernel = kernel(2, 1, 1);
    kernel.register_character(binding(), character()).unwrap();
    kernel.enqueue(command(context(1), 1)).unwrap();
    kernel.enqueue(command(context(2), 1)).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.queued_commands(), 1);
    assert_eq!(kernel.pending_progression_outcomes(), 1);
    assert!(kernel.progression_backpressured());
    kernel.enqueue(command(context(3), 1)).unwrap();
    assert!(matches!(
        kernel.enqueue(command(context(4), 1)),
        Err(SimulationError::QueueFull)
    ));
    kernel.step().unwrap();
    assert_eq!(kernel.ticks(), 2);
    assert_eq!(kernel.queued_commands(), 2);
    assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 1);
    for sequence in [1, 2, 3] {
        let result = kernel.take_progression_outcome().unwrap();
        assert_eq!(result.context.sequence, sequence);
        assert!(result.result.is_ok());
        kernel.step().unwrap();
    }
    assert_eq!(kernel.queued_commands(), 0);
    assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 3);
}

#[test]
fn queued_old_session_request_cannot_mutate_rebound_character() {
    let mut kernel = kernel(8, 1, 8);
    kernel.register_character(binding(), character()).unwrap();
    kernel.enqueue(command(context(1), 10)).unwrap();
    let new_binding = CharacterBinding {
        session: SessionId(10),
        ..binding()
    };
    assert_eq!(
        kernel.take_character(new_binding).unwrap_err(),
        CharacterRegistrationError::OwnershipMismatch
    );
    let owned = kernel.take_character(binding()).unwrap();
    kernel.register_character(new_binding, owned).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.take_progression_outcome().unwrap().result,
        Err(ProgressionActionRejection::OwnershipMismatch)
    );
    assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 0);
    kernel
        .enqueue(command(
            ActionContext {
                session: new_binding.session,
                ..context(1)
            },
            10,
        ))
        .unwrap();
    kernel.step().unwrap();
    assert!(kernel.take_progression_outcome().unwrap().result.is_ok());
}

#[test]
fn unbound_world_entities_cannot_receive_progression() {
    let mut kernel = kernel(1, 1, 1);
    kernel.enqueue(command(context(1), 1)).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.take_progression_outcome().unwrap().result,
        Err(ProgressionActionRejection::NotBound)
    );
}
