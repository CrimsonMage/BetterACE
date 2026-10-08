use super::*;
use bace_character::{
    CharacterProgression, ProgressionTables, RankTable, SkillCosts, SkillTrainingRules,
    TraitProgress, TraitState,
};
use bace_geometry::{Aabb, Vec3};
use bace_types::CellId;

#[tokio::test]
async fn live_owner_capture_freeze_commit_and_reload_preserve_unsaved_state() {
    let (initial, saved, lease, _) = fixture();
    let binding = binding(reference(&SkillSaveOwner::Plain(initial)));
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
            speed: 5.,
            jump_impulse: 5.,
        },
    )
    .unwrap();
    world
        .insert(bace_entity::Actor {
            id: binding.actor,
            cell: CellId(1),
            body,
        })
        .unwrap();
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let progression = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(6),
                experience_spent: 0,
                advancement: SkillAdvancement::Untrained,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        100,
        2,
    )
    .unwrap()
    .with_training(
        Arc::new(
            SkillTrainingRules::new(&[SkillCosts {
                skill: 6,
                trained_cost: 6,
                specialized_cost: 12,
            }])
            .unwrap(),
        ),
        16,
        &[],
    )
    .unwrap();
    let mut kernel = bace_simulation::Kernel::with_gameplay_limits(world, 16, 2, 8).unwrap();
    kernel.register_character(binding, progression).unwrap();
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
    let enchanted_item = add_enchanted_item(&mut kernel, binding);
    let before = kernel.read_player_snapshot(binding).unwrap();
    let baseline = crate::player_saves::freeze_player_snapshot(&saved, &before, 10_000).unwrap();
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    online
        .register(binding, lease, baseline, 3, Duration::ZERO)
        .unwrap();
    online
        .register_inventory_v5(binding.actor.0, vec![(enchanted_item, 4)])
        .unwrap();
    for _ in 0..150 {
        kernel.step().unwrap();
    }
    kernel
        .apply_ui(
            ActionContext {
                sequence: 41,
                ..initial.context
            },
            bace_gameplay_api::UiRequest::Filters(3),
        )
        .unwrap();
    online
        .mark_dirty(binding.actor.0, Duration::from_secs(1))
        .unwrap();
    let ticket = kernel
        .propose_skill(
            initial.context,
            bace_simulation::SkillIntent::Train(bace_gameplay_api::TrainSkill {
                skill: 6,
                quoted_credits: 6,
            }),
        )
        .unwrap();
    assert!(kernel.read_player_snapshot(binding).is_err());
    let mut service = SkillService::new();
    service
        .stage(
            SkillOperationId::new([7; 16]).unwrap(),
            SkillSaveOwner::Plain(ticket),
        )
        .unwrap();
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    service
        .poll_with(&mut online, &worker.handle, 100, |_| panic!())
        .unwrap();
    service
        .poll_with(&mut online, &worker.handle, 100, |command| {
            kernel
                .try_enqueue(command)
                .map_err(|command| Box::new(TrySendError::Full(command)))
        })
        .unwrap();
    kernel.step().unwrap();
    let capture = kernel.take_player_snapshot_outcome().unwrap();
    assert!(
        capture.result.is_ok(),
        "exact skill hold permits immutable capture"
    );
    service.accept_capture(capture, 10_034).unwrap();
    until_receipt(&mut service, &mut online, &worker.handle).await;
    assert!(service.blocked().is_none(), "{:?}", service.blocked());
    service
        .poll_with(&mut online, &worker.handle, 101, |command| {
            kernel
                .try_enqueue(command)
                .map_err(|command| Box::new(TrySendError::Full(command)))
        })
        .unwrap();
    kernel.step().unwrap();
    service
        .accept_skill(kernel.take_skill_outcome().unwrap())
        .unwrap();
    service
        .poll_with(&mut online, &worker.handle, 102, |_| panic!())
        .unwrap();
    assert!(service.take_completion().unwrap().committed);
    let capture = kernel.read_player_snapshot(binding).unwrap();
    {
        let log = backend.seen.lock().unwrap();
        let restored = PlayerSaveV6::decode(&log[0].snapshots[0].bytes).unwrap();
        assert_eq!(
            restored.player.entity.mutation_revision,
            ticket.change.revision
        );
        assert_eq!(
            restored
                .player
                .entity
                .state
                .properties
                .skills
                .iter()
                .find(|s| s.id == 6)
                .unwrap()
                .value
                .sac,
            2
        );
        assert_eq!(restored.ui.spellbook_filters, 3);
        assert_ne!(restored.ui.spellbook_filters, saved.ui.spellbook_filters);
        assert_eq!(
            capture.character().progression().revision(),
            restored.player.entity.mutation_revision
        );
        assert_eq!(online.baseline(binding.actor.0).unwrap().0, &restored);
        let item_row = log[0]
            .snapshots
            .iter()
            .find(|s| s.object_id == 0x80000001)
            .unwrap();
        let item = bace_storage_codec::ItemSaveV5::decode(&item_row.bytes).unwrap();
        assert_eq!(item.source_destination, Some(2));
        assert!(item.enchantments[0].start_time <= -5.0);
        assert!(item.entity.mutation_revision > 1);
        assert_eq!(item_row.expected_version, 4);
        assert_eq!(
            online.inventory_baselines(binding.actor.0)[0].enchantments,
            item.enchantments
        );
        assert_eq!(
            online.inventory_baselines(binding.actor.0)[0].source_destination,
            Some(2)
        );
    }
    worker.handle.close();
    worker.task.await.unwrap();
}

fn add_enchanted_item(
    kernel: &mut bace_simulation::Kernel,
    binding: CharacterBinding,
) -> bace_storage_codec::ItemSaveV5 {
    use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
    use bace_storage_codec::{ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5};
    let id = EntityId(0x80000001);
    kernel
        .register_inventory_container(InventoryContainer {
            id: binding.actor,
            revision: 1,
            root_owner: Some(binding.actor),
            slots: 10,
            pack_slots: 2,
            burden_limit: 10000,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    kernel
        .register_inventory_item(InventoryItem {
            structure: None,
            id,
            revision: 1,
            template: 10,
            stack_key: 10,
            place: ItemPlace::Contained {
                container: binding.actor,
                slot: 0,
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
        })
        .unwrap();
    let entry = bace_magic::EnchantmentEntry {
        spell: 123,
        caster: binding.actor.0,
        school: bace_magic::MagicSchool::Creature,
        spec: bace_magic::EnchantmentSpec {
            category: 23,
            power: 100,
            duration: 100.,
            layer: 3,
            stat_type: 0x1000,
            stat_key: 7,
            value: 12.5,
            beneficial: true,
            set_id: None,
        },
        start_time: 0.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    };
    let registry = bace_magic::EnchantmentRegistry::restore(8, 20, vec![entry.clone()]).unwrap();
    kernel.register_magic_registry(id, registry, true).unwrap();
    let mut saved = ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: EntitySaveV1 {
            object_id: id.0,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 10,
                class_name: "enchanted_skill_fixture".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        placement: ItemPlacementV2::Contained {
            container: binding.actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        },
    })
    .unwrap();
    saved
        .enchantments
        .push(crate::enchantment_saves::freeze_enchantment(&entry).unwrap());
    ItemSaveV5 {
        previous: saved,
        source_destination: Some(2),
    }
}
