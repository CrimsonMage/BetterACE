#[path = "crafting/motion.rs"]
mod motion;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_crafting::*;
use bace_entity::Actor;
use bace_gameplay_api::{ActionContext, CharacterBinding, SessionId};
use bace_geometry::{Aabb, Vec3};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::{InventoryReceipt, Kernel};
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
    let mut combatant = bace_entity::Combatant::new(bace_entity::CombatantProfile {
        maximum_health: 100,
        melee_damage: 1,
        melee_range: 2.0,
        attack_duration: 1.0,
        strike_offsets: vec![0.5],
        player: true,
    })
    .unwrap();
    combatant = combatant
        .with_resources(
            Some(bace_entity::VitalPool {
                current: 10,
                maximum: 100,
            }),
            Some(bace_entity::VitalPool {
                current: 20,
                maximum: 100,
            }),
        )
        .unwrap();
    assert!(combatant.set_mode(1));
    world.register_combatant(EntityId(1), combatant).unwrap();
    let table = RankTable::new(&[0, 10, 100]).unwrap();
    let mut character = CharacterProgression::with_state(
        &[bace_character::TraitState {
            progress: bace_character::TraitProgress {
                target: bace_gameplay_api::ProgressionTarget::Skill(18),
                experience_spent: 0,
                advancement: bace_gameplay_api::SkillAdvancement::Trained,
            },
            details: bace_gameplay_api::TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
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

#[test]
fn crafting_uses_existing_inventory_reservations_and_receipts() {
    let mut k = kernel();
    let ctx = context();
    let source = craft_item(2);
    let target = craft_item(3);
    let r = recipe();
    k.quote_tinker(action(1), &ctx, &source, &target, &r, 30)
        .unwrap();
    let op = k
        .confirm_tinker(action(2), &ctx, &source, &target, &r)
        .unwrap();
    assert!(
        k.take_inventory_proposal().is_none(),
        "generic saver must not swallow crafting properties"
    );
    assert!(k.inventory_item(EntityId(2)).is_some());
    assert_eq!(k.inventory_item(EntityId(3)).unwrap().revision, 1);
    assert!(k.propose_item_take(EntityId(1), EntityId(3), 1).is_err());
    assert!(
        k.confirm_crafting_committed(&InventoryReceipt {
            operation: op,
            revisions: vec![]
        })
        .is_err()
    );
    let ticket = k.take_crafting_proposal().unwrap();
    assert_eq!(ticket.operation, op);
    k.retry_crafting(op).unwrap();
    assert_eq!(k.take_crafting_proposal().unwrap(), ticket);
    let receipt = InventoryReceipt {
        operation: op,
        revisions: ticket
            .inventory
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    let committed = k.confirm_crafting_committed(&receipt).unwrap();
    assert_eq!(committed, ticket);
    assert!(k.inventory_item(EntityId(2)).is_none());
    assert_eq!(k.inventory_item(EntityId(3)).unwrap().revision, 2);
    assert!(matches!(
        k.inventory_item(EntityId(3)).unwrap().place,
        ItemPlace::Contained { slot: 0, .. }
    ));
    assert!(!k.has_crafting_state());
    assert!(k.confirm_crafting_committed(&receipt).is_err());
}

#[test]
fn definite_rollback_retains_items_and_releases_reservations() {
    let mut k = kernel();
    let ctx = context();
    let r = recipe();
    k.quote_tinker(action(1), &ctx, &craft_item(2), &craft_item(3), &r, 30)
        .unwrap();
    let op = k
        .confirm_tinker(action(2), &ctx, &craft_item(2), &craft_item(3), &r)
        .unwrap();
    k.take_crafting_proposal().unwrap();
    k.reject_crafting(op).unwrap();
    assert!(k.inventory_item(EntityId(2)).is_some());
    assert!(k.propose_item_take(EntityId(1), EntityId(3), 1).is_ok());
}

#[test]
fn stale_session_and_repeated_confirmation_cannot_create_second_operation() {
    let mut k = kernel();
    let ctx = context();
    let r = recipe();
    let mut wrong = action(1);
    wrong.session = SessionId(99);
    assert!(
        k.quote_tinker(wrong, &ctx, &craft_item(2), &craft_item(3), &r, 30)
            .is_err()
    );
    k.quote_tinker(action(1), &ctx, &craft_item(2), &craft_item(3), &r, 30)
        .unwrap();
    k.confirm_tinker(action(2), &ctx, &craft_item(2), &craft_item(3), &r)
        .unwrap();
    assert_eq!(
        k.confirm_tinker(action(2), &ctx, &craft_item(2), &craft_item(3), &r),
        Err(CraftError::Busy)
    );
}

fn timed_registry() -> bace_magic::EnchantmentRegistry {
    bace_magic::EnchantmentRegistry::restore(
        8,
        20,
        vec![bace_magic::EnchantmentEntry {
            spell: 123,
            caster: 1,
            school: bace_magic::MagicSchool::Creature,
            spec: bace_magic::EnchantmentSpec {
                category: 23,
                power: 100,
                duration: 100.0,
                layer: 3,
                stat_type: 0x1000,
                stat_key: 7,
                value: 12.5,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: bace_magic::EnchantmentMetadata::default(),
        }],
    )
    .unwrap()
}
#[test]
fn pending_craft_freezes_actor_and_compacted_sibling_registries_until_rollback() {
    let mut k = kernel();
    k.register_inventory_item(item(4, 2)).unwrap();
    for id in 1..=4 {
        k.register_magic_registry(EntityId(id), timed_registry(), true)
            .unwrap();
    }
    let ctx = context();
    let r = recipe();
    k.quote_tinker(action(1), &ctx, &craft_item(2), &craft_item(3), &r, 300)
        .unwrap();
    let op = k
        .confirm_tinker(action(2), &ctx, &craft_item(2), &craft_item(3), &r)
        .unwrap();
    let ticket = k.take_crafting_proposal().unwrap();
    assert_eq!(
        ticket.registry_reservations,
        (1..=4).map(EntityId).collect::<Vec<_>>()
    );
    assert!(k.retry_inventory(op).is_err());
    assert!(k.reject_inventory(op).is_err());
    let receipt = InventoryReceipt {
        operation: op,
        revisions: ticket
            .inventory
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(k.confirm_inventory_committed(&receipt).is_err());
    for _ in 0..151 {
        k.step().unwrap();
    }
    for id in 1..=4 {
        assert_eq!(
            k.magic_registry(EntityId(id)).unwrap().entries()[0].start_time,
            0.0
        );
        assert!(k.take_magic_registry(EntityId(id)).is_err());
    }
    k.reject_crafting(op).unwrap();
    k.step().unwrap();
    for id in 1..=4 {
        assert_eq!(
            k.magic_registry(EntityId(id)).unwrap().entries()[0].start_time,
            -5.0
        );
    }
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 2);
    assert_eq!(k.inventory_item(EntityId(4)).unwrap().revision, 2);
}
#[test]
fn committed_craft_releases_registries_and_retains_elapsed_timer_debt() {
    let mut k = kernel();
    for id in 1..=3 {
        k.register_magic_registry(EntityId(id), timed_registry(), true)
            .unwrap();
    }
    let ctx = context();
    let r = recipe();
    k.quote_tinker(action(1), &ctx, &craft_item(2), &craft_item(3), &r, 300)
        .unwrap();
    let op = k
        .confirm_tinker(action(2), &ctx, &craft_item(2), &craft_item(3), &r)
        .unwrap();
    let ticket = k.take_crafting_proposal().unwrap();
    for _ in 0..151 {
        k.step().unwrap();
    }
    k.confirm_crafting_committed(&InventoryReceipt {
        operation: op,
        revisions: ticket
            .inventory
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    k.step().unwrap();
    assert_eq!(
        k.magic_registry(EntityId(1)).unwrap().entries()[0].start_time,
        -5.0
    );
    assert_eq!(
        k.magic_registry(EntityId(3)).unwrap().entries()[0].start_time,
        -5.0
    );
    assert!(
        k.magic_registry(EntityId(2)).is_none(),
        "durably consumed source must retire its registry"
    );
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 3);
    assert_eq!(k.inventory_item(EntityId(3)).unwrap().revision, 3);
}
#[test]
fn crafting_commands_echo_correlation_and_reject_oversized_inputs() {
    use bace_simulation::{
        CraftingCommand, CraftingCommandKind, CraftingResult, TinkerCommandInput,
    };
    let mut k = kernel();
    let input = TinkerCommandInput {
        context: context(),
        source: craft_item(2),
        target: craft_item(3),
        recipe: Arc::new(recipe()),
    };
    let outcome = k.apply_crafting_command(CraftingCommand {
        correlation: 99,
        action: CraftingCommandKind::Quote {
            context: action(1),
            input: Box::new(input.clone()),
            lifetime: 30,
        },
    });
    assert_eq!(outcome.correlation, 99);
    assert!(matches!(outcome.result, Ok(CraftingResult::Quoted(_))));
    let mut oversized = input.clone();
    oversized.context.properties.insert(
        PropertyKey {
            kind: PropertyKind::String,
            id: 1,
        },
        PropertyValue::String("x".repeat(65537)),
    );
    let command = CraftingCommand {
        correlation: 100,
        action: CraftingCommandKind::Confirm {
            context: action(2),
            input: Box::new(oversized),
        },
    };
    assert_eq!(command.validate_bounds(), Err(CraftError::Capacity));
    assert_eq!(
        k.apply_crafting_command(command).result,
        Err(CraftError::Capacity)
    );
    let result = k.apply_crafting_command(CraftingCommand {
        correlation: 101,
        action: CraftingCommandKind::Confirm {
            context: action(2),
            input: Box::new(input),
        },
    });
    assert!(matches!(result.result, Ok(CraftingResult::Pending(_))));
}

#[test]
fn salvage_command_reserves_ust_and_commits_all_outputs_with_timers() {
    use bace_simulation::{
        CraftingCommand, CraftingCommandKind, CraftingResult, SalvageCommandInput,
    };
    let mut k = kernel();
    for id in 1..=3 {
        k.register_magic_registry(EntityId(id), timed_registry(), true)
            .unwrap();
    }
    let mut generated = item(4, 2);
    generated.template = 123;
    generated.unit_value = 1;
    generated.structure = Some(1);
    let outcome = k.apply_crafting_command(CraftingCommand {
        correlation: 1,
        action: CraftingCommandKind::Salvage {
            context: action(1),
            input: Box::new(SalvageCommandInput {
                actor_revision: 1,
                operation_id: [9; 16],
                tool: SalvageTool {
                    id: 2,
                    owner: 1,
                    revision: 1,
                    is_ust: true,
                    reserved: false,
                },
                skills: SalvageSkills {
                    salvaging: 0,
                    armor: 0,
                    weapon: 0,
                    magic_item: 0,
                    item: 0,
                    augmentations: 0,
                },
                items: vec![SalvageInput {
                    id: 3,
                    owner: 1,
                    revision: 1,
                    stack: 1,
                    material: 61,
                    raw_workmanship: 10,
                    value: 100,
                    retained: false,
                    equipped: false,
                    in_trade: false,
                    reserved: false,
                    is_salvage: false,
                    structure: 0,
                    num_items: 0,
                }],
                bag_templates: Arc::new(BTreeMap::from([(61, 123)])),
                generated: vec![generated],
            }),
        },
    });
    let Ok(CraftingResult::Pending(operation)) = outcome.result else {
        panic!("{outcome:?}")
    };
    let ticket = k.take_crafting_proposal().unwrap();
    assert!(
        ticket
            .inventory
            .participants
            .iter()
            .any(|(id, _)| *id == EntityId(2))
    );
    assert!(k.propose_item_take(EntityId(1), EntityId(2), 1).is_err());
    for _ in 0..151 {
        k.step().unwrap();
    }
    assert_eq!(
        k.magic_registry(EntityId(2)).unwrap().entries()[0].start_time,
        0.0
    );
    let outcome = k.apply_crafting_command(CraftingCommand {
        correlation: 2,
        action: CraftingCommandKind::Commit {
            receipt: InventoryReceipt {
                operation,
                revisions: ticket
                    .inventory
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            },
        },
    });
    assert!(matches!(outcome.result, Ok(CraftingResult::Committed(_))));
    assert!(k.inventory_item(EntityId(3)).is_none());
    assert_eq!(k.inventory_item(EntityId(4)).unwrap().template, 123);
    assert!(
        k.magic_registry(EntityId(3)).is_none(),
        "durably salvaged item must retire its registry"
    );
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 2);
}

#[test]
fn recipe_actor_edits_require_their_live_owner_before_any_reservation() {
    let mut kernel = kernel();
    let ctx = context();
    let source = craft_item(2);
    let target = craft_item(3);
    let mut unsupported = recipe();
    unsupported.success.mutations.push(Mutation {
        participant: Participant::Actor,
        key: PropertyKey {
            kind: PropertyKind::Int,
            id: 24,
        },
        kind: MutationKind::Add(PropertyValue::Int(10)),
    });
    assert_eq!(
        kernel.quote_tinker(action(1), &ctx, &source, &target, &unsupported, 30),
        Err(CraftError::Unsupported)
    );
    assert!(!kernel.has_crafting_state());
    // Rejected prepared input did not consume the authenticated sequence or
    // block a valid ordinary tinker/imbue statistic update.
    let supported = recipe();
    kernel
        .quote_tinker(action(1), &ctx, &source, &target, &supported, 30)
        .unwrap();
    let operation = kernel
        .confirm_tinker(action(2), &ctx, &source, &target, &supported)
        .unwrap();
    kernel.reject_crafting(operation).unwrap();
}

#[test]
fn no_dialog_tinker_reorders_modified_item_and_retains_sibling_revision_until_receipt() {
    let mut k = kernel();
    k.register_inventory_item(item(4, 2)).unwrap();
    let mut r = recipe();
    r.success.consume_source = 0;
    r.success.destroy_source_chance = 0.0;
    let op = k
        .execute_tinker(action(1), &context(), &craft_item(2), &craft_item(3), &r)
        .unwrap();
    let ticket = k.take_crafting_proposal().unwrap();
    assert_eq!(ticket.operation, op);
    assert_eq!(
        k.inventory_item(EntityId(3)).unwrap().place,
        item(3, 1).place
    );
    let after = |id| {
        ticket
            .inventory
            .changes
            .iter()
            .find(|c| c.after.id == EntityId(id))
            .map(|c| &c.after)
    };
    assert_eq!(after(3).unwrap().place, item(3, 0).place);
    assert_eq!(after(2).unwrap().place, item(2, 1).place);
    assert!(
        after(4).is_none(),
        "unchanged later sibling must not get a revision"
    );
    let receipt = InventoryReceipt {
        operation: op,
        revisions: ticket
            .inventory
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    k.confirm_crafting_committed(&receipt).unwrap();
    assert_eq!(
        k.inventory_item(EntityId(3)).unwrap().place,
        item(3, 0).place
    );
    assert_eq!(k.inventory_item(EntityId(2)).unwrap().revision, 2);
    assert_eq!(
        k.execute_tinker(action(1), &context(), &craft_item(2), &craft_item(3), &r),
        Err(CraftError::Ownership)
    );
}

#[test]
fn proficiency_is_part_of_exact_craft_receipt_and_rollback_retains_skill_usage() {
    let mut k = proficiency_kernel(0);
    let mut recipe = recipe();
    recipe.proficiency = Some((18, 5));
    let before = k
        .character(EntityId(1))
        .unwrap()
        .projection(bace_gameplay_api::ProgressionTarget::Skill(18))
        .unwrap();
    let op = k
        .execute_tinker(
            action(1),
            &context(),
            &craft_item(2),
            &craft_item(3),
            &recipe,
        )
        .unwrap();
    let ticket = k.take_crafting_proposal().unwrap();
    let patch = ticket.proficiency.as_ref().unwrap();
    assert_eq!((patch.change.spent, patch.change.earned.credited), (5, 6));
    assert_eq!(
        k.character(EntityId(1)).unwrap().projection(before.target),
        Some(before)
    );
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_experience(),
        100
    );
    k.reject_crafting(op).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().projection(before.target),
        Some(before)
    );
    let op = k
        .execute_tinker(
            action(2),
            &context(),
            &craft_item(2),
            &craft_item(3),
            &recipe,
        )
        .unwrap();
    let ticket = k.take_crafting_proposal().unwrap();
    let receipt = InventoryReceipt {
        operation: op,
        revisions: ticket
            .inventory
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    k.confirm_crafting_committed(&receipt).unwrap();
    let c = k.character(EntityId(1)).unwrap();
    assert_eq!(c.available_experience(), 101);
    assert_eq!(c.projection(before.target).unwrap().experience_spent, 5);
    assert_eq!(c.revision(), 2);
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_experience,
        6
    );
    assert!(k.confirm_crafting_committed(&receipt).is_err());
}

fn proficiency_kernel(total: u64) -> Kernel {
    let mut k = kernel();
    k.set_social_epoch(10000).unwrap();
    k.register_social_presence(
        bace_social::SocialPresence {
            identity: bace_gameplay_api::social::SocialIdentity {
                character: EntityId(1),
                account: AccountId(1),
                name: "Crafter".into(),
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
        bace_social::SocialPreferences::default(),
    )
    .unwrap();
    k.register_npc_character_services(
        EntityId(1),
        bace_character::CharacterServiceState {
            level: 1,
            total_experience: total,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
            total_skill_credits: Some(0),
        },
        bace_quests::ContractRegistry::restore(vec![]).unwrap(),
    )
    .unwrap();
    k.configure_social_experience(Arc::new(
        bace_character::CharacterLevelTable::prepare(vec![0, 0, 10000], vec![0; 3]).unwrap(),
    ))
    .unwrap();
    k
}

#[test]
fn proficiency_level_and_vitae_are_reserved_and_adopted_with_same_craft() {
    let mut k = proficiency_kernel(9998);
    let mut entry = timed_registry().entries()[0].clone();
    entry.spell = 666;
    entry.spec.category = 666;
    entry.spec.value = 0.95;
    entry.spec.duration = -1.;
    k.register_magic_registry(
        EntityId(1),
        bace_magic::EnchantmentRegistry::restore(8, 20, vec![entry]).unwrap(),
        true,
    )
    .unwrap();
    k.register_player_death_state(
        EntityId(1),
        bace_simulation::PlayerDeathState {
            death_level: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut recipe = recipe();
    recipe.proficiency = Some((18, 5));
    let op = k
        .execute_tinker(
            action(1),
            &context(),
            &craft_item(2),
            &craft_item(3),
            &recipe,
        )
        .unwrap();
    let ticket = k.take_crafting_proposal().unwrap();
    let patch = ticket.proficiency.as_ref().unwrap();
    assert_eq!(patch.change.earned.credited, 2);
    assert_eq!(patch.vitals.len(), 3);
    assert!(patch.vitae.is_some());
    assert_eq!(
        k.world()
            .vital(EntityId(1), bace_entity::EntityVital::Stamina)
            .unwrap()
            .current,
        10
    );
    assert_eq!(k.player_death_state(EntityId(1)).unwrap().vitae_pool, 0);
    assert!(k.world().has_reserved_vitals(EntityId(1)));
    k.confirm_crafting_committed(&InventoryReceipt {
        operation: op,
        revisions: ticket
            .inventory
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    assert_eq!(k.npc_character_services(EntityId(1)).unwrap().level, 2);
    assert_eq!(
        k.world()
            .vital(EntityId(1), bace_entity::EntityVital::Stamina)
            .unwrap()
            .current,
        100
    );
    assert_eq!(
        k.player_death_state(EntityId(1)),
        Some(&patch.vitae.as_ref().unwrap().after)
    );
    assert!(!k.world().has_reserved_vitals(EntityId(1)));
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 2);
    let registry_revision = k.magic_registry(EntityId(1)).unwrap().revision();
    k.step().unwrap();
    let timer_changed = k.magic_registry(EntityId(1)).unwrap().revision() != registry_revision;
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        2 + u64::from(timer_changed),
        "committed vitals are already accounted for; only a later timer change dirties the craft"
    );
}
