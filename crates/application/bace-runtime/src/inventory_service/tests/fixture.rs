use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_content::Property;
use bace_gameplay_api::{ActionContext, SessionId};
use bace_geometry::{Aabb, Vec3};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_storage_codec::EntitySaveV1;
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
            class_name: "inventory_service_fixture".into(),
            weenie_type: 51,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn saved_item(id: EntityId, slot: u32) -> ItemSaveV4 {
    let mut entity = entity(id);
    entity.state.properties.ints.extend([
        Property { id: 11, value: 100 },
        Property { id: 12, value: 5 },
        Property { id: 13, value: 1 },
        Property { id: 15, value: 100 },
    ]);
    entity.state.properties.strings.push(Property {
        id: 16,
        value: "instance-only mutation".into(),
    });
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
        stack: 5,
        maximum_stack: 5,
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
pub(super) fn fixture() -> (
    bace_simulation::Kernel,
    OnlinePlayerSaveService,
    InventoryWork,
) {
    fixture_with_dirty_ui(true)
}
pub(super) fn fixture_with_dirty_ui(
    dirty: bool,
) -> (
    bace_simulation::Kernel,
    OnlinePlayerSaveService,
    InventoryWork,
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
    let saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: entity(ACTOR),
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
    let mut template = source.entity.state.clone();
    template.properties.strings.clear();
    let authority = bace_inventory::InventoryAuthority {
        actor: ACTOR,
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: false,
        source_view: None,
        destination_view: None,
        new_item: Some(FRESH),
    };
    let (prepared, fresh) = prepare_split_request(SplitPreparationInput {
        context: action(2),
        request: bace_gameplay_api::InventoryRequest::SplitToContainer {
            item: SOURCE,
            container: ACTOR,
            placement: 2,
            amount: 2,
        },
        authority,
        template: &template,
        template_revision: 1,
        fresh_id: FRESH,
        source: &source.entity.state,
        source_vendor: false,
        destination_corpse: false,
        wield_requirements_met: true,
        drop: None,
    })
    .unwrap();
    kernel
        .try_enqueue(Command::Inventory(Box::new(InventoryCommand {
            correlation: 2,
            kind: InventoryCommandKind::Propose(Box::new(prepared)),
        })))
        .ok()
        .unwrap();
    kernel.step().unwrap();
    let outcome = kernel.take_inventory_outcome().unwrap();
    let InventoryDecision::Proposed(operation) = outcome.result.unwrap() else {
        panic!("proposal expected")
    };
    assert!(
        kernel.take_inventory_proposal().is_none(),
        "claimed command must not race the generic FIFO"
    );
    (
        kernel,
        online,
        InventoryWork {
            binding,
            operation: *operation,
            fresh: Some(fresh),
            external: vec![],
            storage_views: vec![],
            world_epoch: 1,
        },
    )
}
pub(super) const FRESH: EntityId = EntityId(0x80000003);
