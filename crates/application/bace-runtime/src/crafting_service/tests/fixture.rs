use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_content::Property;
use bace_crafting::*;
use bace_gameplay_api::{ActionContext, SessionId};
use bace_geometry::{Aabb, Vec3};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_storage_codec::{ItemPlacementV2, ItemSaveV2, ItemSaveV4, PlayerSaveV1, PlayerSaveV6};
use bace_types::{AccountId, CellId, EntityId};
const ACTOR: EntityId = EntityId(0x50000001);
pub(super) const SOURCE: EntityId = EntityId(0x80000001);
pub(super) const TARGET: EntityId = EntityId(0x80000002);
fn entity(id: EntityId) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id.0,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 10,
            class_name: "craft_service_fixture".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn saved_item(id: EntityId, slot: u32) -> ItemSaveV4 {
    let mut entity = entity(id);
    entity
        .state
        .properties
        .ints
        .push(Property { id: 28, value: 10 });
    ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity,
        placement: ItemPlacementV2::Contained {
            container: ACTOR.0,
            slot,
            pack_slot: false,
            equipped: 0,
        },
    })
    .unwrap()
}
fn item(id: EntityId, slot: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id,
        revision: 1,
        template: 10,
        stack_key: 10,
        place: ItemPlace::Contained {
            container: ACTOR,
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
pub(crate) fn fixture() -> (
    bace_simulation::Kernel,
    OnlinePlayerSaveService,
    CraftingWork,
) {
    fixture_with_dirty_ui(true)
}
pub(super) fn fixture_with_dirty_ui(
    dirty: bool,
) -> (
    bace_simulation::Kernel,
    OnlinePlayerSaveService,
    CraftingWork,
) {
    let binding = CharacterBinding {
        actor: ACTOR,
        account: AccountId(1),
        session: SessionId(1),
    };
    let action = |sequence| ActionContext {
        actor: ACTOR,
        account: binding.account,
        session: binding.session,
        sequence,
    };
    let mut world = bace_world::World::default();
    world
        .register_scene(
            CellId(1),
            bace_physics::SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let body = bace_physics::Body::spawn(
        world.scene(CellId(1)).unwrap(),
        Vec3::new(0., 0., 0.5),
        0.5,
        bace_motion::Capabilities {
            speed: 1.,
            jump_impulse: 1.,
        },
    )
    .unwrap();
    world
        .insert(bace_entity::Actor {
            id: ACTOR,
            cell: CellId(1),
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
    .unwrap()
    .with_resources(
        Some(bace_entity::VitalPool {
            current: 100,
            maximum: 100,
        }),
        Some(bace_entity::VitalPool {
            current: 100,
            maximum: 100,
        }),
    )
    .unwrap();
    assert!(combatant.set_mode(1));
    world.register_combatant(ACTOR, combatant).unwrap();
    let table = RankTable::new(&[0, 10, 100]).unwrap();
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
    character = character
        .with_training(
            Arc::new(bace_character::SkillTrainingRules::new(&[]).unwrap()),
            0,
            &[],
        )
        .unwrap();
    character.touch_revision().unwrap();
    character.touch_revision().unwrap();
    let mut kernel = bace_simulation::Kernel::new(world, 16).unwrap();
    kernel.register_character(binding, character).unwrap();
    let mut player_entity = entity(ACTOR);
    player_entity.state.properties.secondary_attributes = [1, 3, 5]
        .into_iter()
        .map(|id| Property {
            id,
            value: bace_content::SecondaryAttribute {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 100,
            },
        })
        .collect();
    let saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: player_entity,
        account_id: 1,
        name: "Crafter".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    kernel
        .register_character_ui(
            binding,
            bace_simulation::OwnedUiState {
                state: crate::ui_saves::restore_ui(&saved).unwrap(),
                known_spells: vec![],
                component_templates: vec![],
                entered: true,
            },
        )
        .unwrap();
    kernel
        .register_inventory_container(InventoryContainer {
            id: ACTOR,
            revision: 1,
            root_owner: Some(ACTOR),
            slots: 10,
            pack_slots: 2,
            burden_limit: 10000,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    kernel.register_inventory_item(item(SOURCE, 0)).unwrap();
    kernel.register_inventory_item(item(TARGET, 1)).unwrap();
    kernel
        .configure_crafting_random(Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()))
        .unwrap();
    let before = kernel.read_player_snapshot(binding).unwrap();
    let saved = crate::player_saves::freeze_player_snapshot(&saved, &before, 10_000).unwrap();
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    online
        .register(
            binding,
            CharacterLease {
                character_id: ACTOR.0,
                epoch: 1,
                state: OwnershipState::Online,
            },
            saved.clone(),
            3,
            Duration::ZERO,
        )
        .unwrap();
    let source = saved_item(SOURCE, 0);
    let target = saved_item(TARGET, 1);
    online
        .register_inventory(ACTOR.0, vec![(source.clone(), 4), (target.clone(), 4)])
        .unwrap();
    if dirty {
        kernel
            .apply_ui(action(1), bace_gameplay_api::UiRequest::Filters(3))
            .unwrap();
        online.mark_dirty(ACTOR.0, Duration::from_secs(1)).unwrap();
    }
    let current = kernel.read_player_snapshot(binding).unwrap();
    let current = crate::player_saves::freeze_player_snapshot(&saved, &current, 10_000).unwrap();
    let context = CraftContext {
        actor: ACTOR.0,
        actor_revision: current.player.entity.mutation_revision,
        character_random_id: [1; 16],
        operation_id: [2; 16],
        busy: false,
        peace_mode: true,
        chance: ChanceInput {
            skill: 1000,
            trained: true,
            lum_craft: 0,
            tool_workmanship: 10.,
            target_workmanship: 10.,
            material: 61,
            times_tinkered: 0,
            imbue: dirty,
            imbue_augmentation: false,
            foolproof: true,
        },
        properties: crate::crafting_saves::crafting_properties(
            &current.player.entity.state.properties,
        ),
    };
    let source = crate::crafting_saves::craft_item_snapshot(&source, ACTOR.0).unwrap();
    let target = crate::crafting_saves::craft_item_snapshot(&target, ACTOR.0).unwrap();
    let branch = RecipeBranch {
        consume_source: 1,
        consume_target: 0,
        destroy_source_chance: 1.,
        destroy_target_chance: 0.,
        mutations: vec![Mutation {
            participant: Participant::Target,
            key: PropertyKey {
                kind: PropertyKind::Int,
                id: 28,
            },
            kind: MutationKind::Add(PropertyValue::Int(1)),
        }],
    };
    let recipe = PreparedRecipe {
        id: 1,
        revision: 1,
        requirements: vec![],
        success: branch.clone(),
        failure: branch,
        increment_tinker_count: true,
        proficiency: None,
    };
    kernel
        .quote_tinker(action(2), &context, &source, &target, &recipe, 30)
        .unwrap();
    kernel
        .confirm_tinker(action(3), &context, &source, &target, &recipe)
        .unwrap();
    let ticket = kernel.take_crafting_proposal().unwrap();
    (
        kernel,
        online,
        CraftingWork {
            binding,
            ticket,
            generated: vec![],
        },
    )
}
pub(super) fn salvage_fixture() -> (
    bace_simulation::Kernel,
    OnlinePlayerSaveService,
    CraftingWork,
) {
    let (mut kernel, online, work) = fixture_with_dirty_ui(false);
    kernel.reject_crafting(work.ticket.operation).unwrap();
    let binding = work.binding;
    let generated_id = EntityId(0x80000003);
    let mut generated = item(generated_id, 1);
    generated.template = 123;
    generated.unit_value = 1;
    generated.structure = Some(1);
    let source = [SalvageInput {
        id: TARGET.0,
        owner: ACTOR.0,
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
    }];
    let templates = std::collections::BTreeMap::from([(61, 123)]);
    let request = SalvageRequest {
        actor: ACTOR.0,
        actor_revision: kernel.character(ACTOR).unwrap().revision(),
        operation_id: [9; 16],
        tool: SalvageTool {
            id: SOURCE.0,
            owner: ACTOR.0,
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
        items: &source,
        bag_templates: &templates,
        free_slots: 0,
    };
    kernel
        .propose_salvage(
            ActionContext {
                actor: ACTOR,
                account: binding.account,
                session: binding.session,
                sequence: 4,
            },
            request,
            &[generated],
        )
        .unwrap();
    let ticket = kernel.take_crafting_proposal().unwrap();
    let mut template = entity(generated_id);
    template.state.weenie_id = 123;
    (
        kernel,
        online,
        CraftingWork {
            binding,
            ticket,
            generated: vec![template],
        },
    )
}
