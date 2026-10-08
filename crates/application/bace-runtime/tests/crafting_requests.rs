use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_crafting::*;
use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_gameplay_api::{ActionContext, CharacterBinding, SessionId};
use bace_geometry::{Aabb, Vec3};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::Kernel;
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;
use std::{collections::BTreeMap, sync::Arc};
fn action(sequence: u32) -> ActionContext {
    ActionContext {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
        sequence,
    }
}
fn item(id: u32, slot: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 10,
        stack_key: 10,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 100,
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
    }
}
fn kernel() -> Kernel {
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
    let body = Body::spawn(
        world.scene(cell).unwrap(),
        Vec3::new(1.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 1.0,
            jump_impulse: 1.0,
        },
    )
    .unwrap();
    world
        .insert(Actor {
            id: EntityId(1),
            cell,
            body,
        })
        .unwrap();
    let table = RankTable::new(&[0, 10, 100]).unwrap();
    world
        .register_combatant(
            EntityId(1),
            Combatant::new(CombatantProfile {
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
    let mut character = CharacterProgression::new(
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
    .unwrap();
    character.touch_revision().unwrap();
    let mut kernel = Kernel::new(world, 16).unwrap();
    kernel
        .register_character(
            CharacterBinding {
                actor: EntityId(1),
                account: AccountId(1),
                session: SessionId(1),
            },
            character,
        )
        .unwrap();
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(1),
            revision: 1,
            root_owner: Some(EntityId(1)),
            slots: 10,
            pack_slots: 2,
            burden_limit: 10000,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    kernel.register_inventory_item(item(2, 0)).unwrap();
    kernel.register_inventory_item(item(3, 1)).unwrap();
    kernel
        .configure_crafting_random(Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()))
        .unwrap();
    kernel
}
fn craft_item(id: u32) -> CraftItem {
    CraftItem {
        id,
        owner: 1,
        revision: 1,
        stack: 1,
        equipped: false,
        in_trade: false,
        reserved: false,
        times_tinkered: 0,
        tinker_log: vec![],
        properties: BTreeMap::from([(
            PropertyKey {
                kind: PropertyKind::Int,
                id: 28,
            },
            PropertyValue::Int(10),
        )]),
    }
}
fn context() -> CraftContext {
    CraftContext {
        actor: 1,
        actor_revision: 1,
        character_random_id: [1; 16],
        operation_id: [1; 16],
        busy: false,
        peace_mode: true,
        chance: ChanceInput {
            skill: 1000,
            trained: true,
            lum_craft: 0,
            tool_workmanship: 10.0,
            target_workmanship: 10.0,
            material: 61,
            times_tinkered: 0,
            imbue: true,
            imbue_augmentation: false,
            foolproof: true,
        },
        properties: BTreeMap::new(),
    }
}
fn recipe() -> PreparedRecipe {
    let branch = RecipeBranch {
        consume_source: 1,
        consume_target: 0,
        destroy_source_chance: 1.0,
        destroy_target_chance: 0.0,
        mutations: vec![Mutation {
            participant: Participant::Target,
            key: PropertyKey {
                kind: PropertyKind::Int,
                id: 28,
            },
            kind: MutationKind::Add(PropertyValue::Int(1)),
        }],
    };
    PreparedRecipe {
        id: 1,
        revision: 1,
        requirements: vec![],
        success: branch.clone(),
        failure: branch,
        increment_tinker_count: true,
        proficiency: None,
    }
}

use bace_runtime::crafting_requests::{
    CraftingAdmissionError, CraftingConfirmations, CraftingRequestError,
};
use bace_simulation::{CraftingCommand, CraftingCommandKind, CraftingResult, TinkerCommandInput};
fn decoded(token: u32, session: SessionId) -> bace_session::DispatchedCrafting {
    let payload = [5u32.to_le_bytes(), token.to_le_bytes(), 1u32.to_le_bytes()].concat();
    let bytes = bace_wire::GameActionEnvelope {
        sequence: 2,
        action: bace_wire::opcode::GameActionType::ConfirmationResponse,
        payload: &payload,
    }
    .encode(128)
    .unwrap();
    bace_session::decode_crafting(
        bace_session::SessionState::WorldConnected,
        ActionContext {
            session,
            ..action(2)
        },
        &bytes,
        128,
    )
    .unwrap()
}
#[test]
fn decoded_confirmation_is_bound_to_exact_quote_and_consumed_only_after_admission() {
    let mut kernel = kernel();
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    };
    let input = TinkerCommandInput {
        context: context(),
        source: craft_item(2),
        target: craft_item(3),
        recipe: Arc::new(recipe()),
    };
    let outcome = kernel.apply_crafting_command(CraftingCommand {
        correlation: 77,
        action: CraftingCommandKind::Quote {
            context: action(1),
            input: Box::new(input.clone()),
            lifetime: 30,
        },
    });
    assert!(matches!(outcome.result, Ok(CraftingResult::Quoted(_))));
    let mut confirmations = CraftingConfirmations::new(4);
    let token = confirmations
        .register_quoted(binding, Box::new(input), 0, 30, 77, &outcome)
        .unwrap();
    let reject = |_: CraftingCommand| -> Result<(), &'static str> {
        panic!("invalid response must never reach simulation admission")
    };
    assert_eq!(
        confirmations.submit_response(&decoded(token + 1, SessionId(1)), 1, 78, reject),
        Err(CraftingAdmissionError::Request(
            CraftingRequestError::InvalidConfirmation
        ))
    );
    assert_eq!(
        confirmations.submit_response(&decoded(token, SessionId(2)), 1, 78, reject),
        Err(CraftingAdmissionError::Request(
            CraftingRequestError::WrongBinding
        ))
    );
    assert_eq!(
        confirmations.submit_response(&decoded(token, SessionId(1)), 30, 78, reject),
        Err(CraftingAdmissionError::Request(
            CraftingRequestError::Expired
        ))
    );
    assert_eq!(confirmations.len(), 1);
    assert!(kernel.take_crafting_proposal().is_none());
    assert_eq!(
        confirmations.submit_response(&decoded(token, SessionId(1)), 1, 78, |_| Err("queue full")),
        Err(CraftingAdmissionError::Admission("queue full"))
    );
    assert_eq!(confirmations.len(), 1);
    confirmations
        .submit_response(&decoded(token, SessionId(1)), 1, 78, |command| {
            kernel.enqueue(bace_simulation::Command::Crafting(command))
        })
        .unwrap();
    assert!(confirmations.is_empty());
    kernel.step().unwrap();
    assert!(matches!(
        kernel.take_crafting_outcome().unwrap().result,
        Ok(CraftingResult::Pending(_))
    ));
    assert!(kernel.take_crafting_proposal().is_some());
    assert_eq!(
        confirmations.submit_response(&decoded(token, SessionId(1)), 1, 79, reject),
        Err(CraftingAdmissionError::Request(
            CraftingRequestError::InvalidConfirmation
        ))
    );
}
#[test]
fn replaced_quote_and_old_generation_cleanup_cannot_select_current_inputs() {
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    };
    let mut confirmations = CraftingConfirmations::new(1);
    let input = TinkerCommandInput {
        context: context(),
        source: craft_item(2),
        target: craft_item(3),
        recipe: Arc::new(recipe()),
    };
    let outcome = bace_simulation::CraftingOutcome {
        correlation: 1,
        result: Ok(CraftingResult::Quoted(
            tinker_chance(input.context.chance).unwrap(),
        )),
    };
    let old = confirmations
        .register_quoted(binding, Box::new(input.clone()), 0, 30, 1, &outcome)
        .unwrap();
    let current_binding = CharacterBinding {
        session: SessionId(2),
        ..binding
    };
    let mut current = input;
    current.context.operation_id = [2; 16];
    let token = confirmations
        .register_quoted(current_binding, Box::new(current), 0, 30, 1, &outcome)
        .unwrap();
    assert_ne!(old, token);
    assert_eq!(confirmations.len(), 1);
    confirmations.invalidate(binding);
    assert_eq!(confirmations.len(), 1);
    assert!(
        confirmations
            .submit_response(&decoded(old, SessionId(1)), 1, 2, |_| Ok::<_, ()>(()))
            .is_err()
    );
    confirmations
        .submit_response(&decoded(token, SessionId(2)), 1, 2, |command| {
            let CraftingCommandKind::Confirm { input, .. } = command.action else {
                panic!()
            };
            assert_eq!(input.context.operation_id, [2; 16]);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(confirmations.is_empty());
}
