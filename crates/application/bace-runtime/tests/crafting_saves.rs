use bace_content::{Property, WeenieV1};
use bace_crafting::*;
use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::crafting_saves::*;
use bace_storage_codec::*;
use bace_types::EntityId;
use std::collections::BTreeMap;

const ACTOR: u32 = 0x50000001;

fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: WeenieV1 {
            schema_version: 1,
            weenie_id: 10,
            class_name: "test".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn enchantment() -> FrozenEnchantmentV1 {
    FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 0,
        spell_id: 1,
        layer_id: 1,
        has_spell_set_id: false,
        spell_category: 1,
        power_level: 1,
        start_time: 0.0,
        duration: 30.0,
        caster_object_id: 1,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        stat_mod_type: 0,
        stat_mod_key: 0,
        stat_mod_value: 1.0,
        spell_set_id: 0,
    }
}
fn item(id: u32, slot: u32) -> CraftingSavedItem {
    let mut saved = ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: entity(id),
        placement: ItemPlacementV2::Contained {
            container: ACTOR,
            slot,
            pack_slot: false,
            equipped: 0,
        },
    })
    .unwrap();
    saved.enchantments.push(enchantment());
    saved
        .entity
        .state
        .properties
        .ints
        .push(Property { id: 28, value: 10 });
    saved
        .entity
        .state
        .properties
        .spell_book
        .push(Property { id: 1, value: 1.0 });
    CraftingSavedItem {
        saved: ItemSaveV5 {
            previous: saved,
            source_destination: Some(2),
        },
        persisted_version: 5,
    }
}
fn player() -> CraftingSavedPlayer {
    let mut saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: entity(ACTOR),
        account_id: 1,
        name: "Player".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    saved.ui.gameplay_options = vec![4, 3, 2, 1];
    saved.enchantments.push(enchantment());
    CraftingSavedPlayer {
        saved,
        persisted_version: 7,
    }
}
fn lease() -> CharacterLease {
    CharacterLease {
        character_id: ACTOR,
        epoch: 8,
        state: OwnershipState::Online,
    }
}
fn graph(id: u32, slot: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 10,
        stack_key: 10,
        place: ItemPlace::Contained {
            container: EntityId(ACTOR),
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
fn craft(
    source: &CraftingSavedItem,
    target: &CraftingSavedItem,
    player: &CraftingSavedPlayer,
) -> CraftProposal {
    let source = craft_item_snapshot(&source.saved, ACTOR).unwrap();
    let target = craft_item_snapshot(&target.saved, ACTOR).unwrap();
    let ctx = CraftContext {
        actor: ACTOR,
        actor_revision: 1,
        character_random_id: [1; 16],
        operation_id: [2; 16],
        busy: false,
        peace_mode: true,
        chance: ChanceInput {
            skill: 1000,
            trained: true,
            lum_craft: 0,
            tool_workmanship: 10.0,
            target_workmanship: 10.0,
            material: 0x3d,
            times_tinkered: 0,
            imbue: true,
            imbue_augmentation: false,
            foolproof: true,
        },
        properties: crafting_properties(&player.saved.player.entity.state.properties),
    };
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
    let recipe = PreparedRecipe {
        id: 1,
        revision: 1,
        requirements: vec![],
        success: branch.clone(),
        failure: branch,
        increment_tinker_count: true,
        proficiency: None,
    };
    let quote = quote_craft(&ctx, &source, &target, &recipe, 0, 30).unwrap();
    propose_craft(
        &ctx,
        &source,
        &target,
        &recipe,
        &quote,
        0,
        &bace_random::RandomRoot::new([2; 32], 1).unwrap(),
    )
    .unwrap()
}
#[test]
fn craft_freeze_retains_ui_registry_unknown_properties_and_slot_changes() {
    let source = item(2, 0);
    let mut target = item(3, 1);
    target.saved.entity.state.properties.strings.push(Property {
        id: 9999,
        value: "keep".into(),
    });
    let player = player();
    let proposal = craft(&source, &target, &player);
    let mut removed = graph(2, 0);
    removed.revision = 2;
    removed.stack = 0;
    removed.place = ItemPlace::Removed;
    let mut moved = graph(3, 0);
    moved.revision = 2;
    let inventory = InventoryProposal {
        changes: vec![
            ItemChange {
                before: Some(graph(2, 0)),
                after: removed,
            },
            ItemChange {
                before: Some(graph(3, 1)),
                after: moved,
            },
        ],
        participants: vec![(EntityId(ACTOR), 1), (EntityId(2), 1), (EntityId(3), 1)],
        actor_burden: 1,
        requires_pickup_motion: false,
    };
    let operation = freeze_craft(CraftFreezeInput {
        proposal: &proposal,
        player: &player,
        source: &source,
        target: &target,
        participants: &[ACTOR, 2, 3],
        lease: lease(),
        inventory: &inventory,
        other_items: &[],
    })
    .unwrap();
    assert_eq!(operation.snapshots.len(), 3);
    assert_eq!(
        operation.operation_id,
        "craft-02020202020202020202020202020202"
    );
    let p = PlayerSaveV6::decode(
        &operation
            .snapshots
            .iter()
            .find(|s| s.object_id == ACTOR)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(p.ui, player.saved.ui);
    assert_eq!(p.enchantments, player.saved.enchantments);
    assert_eq!(p.player.entity.mutation_revision, 2);
    let updated = ItemSaveV5::decode(
        &operation
            .snapshots
            .iter()
            .find(|s| s.object_id == 3)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(updated.source_destination, target.saved.source_destination);
    assert_eq!(updated.enchantments, target.saved.enchantments);
    assert_eq!(
        updated.entity.state.properties.spell_book,
        target.saved.entity.state.properties.spell_book
    );
    assert_eq!(
        updated
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 9999)
            .unwrap()
            .value,
        "keep"
    );
    assert_eq!(
        updated
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 28)
            .unwrap()
            .value,
        11
    );
    assert_eq!(
        updated
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 171)
            .unwrap()
            .value,
        1
    );
    assert!(matches!(
        updated.placement,
        ItemPlacementV2::Contained { slot: 0, .. }
    ));
    let deleted = ItemSaveV5::decode(
        &operation
            .snapshots
            .iter()
            .find(|s| s.object_id == 2)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(deleted.source_destination, source.saved.source_destination);
    assert_eq!(deleted.placement, ItemPlacementV2::Removed);
    assert_eq!(deleted.enchantments, source.saved.enchantments);
}

#[test]
fn salvage_freeze_keeps_shifted_sibling_and_creates_every_bag() {
    let source = item(2, 0);
    let sibling = item(3, 1);
    let player = player();
    let inputs = [SalvageInput {
        id: 2,
        owner: ACTOR,
        revision: 1,
        stack: 1,
        material: 61,
        raw_workmanship: 100,
        value: 100,
        retained: false,
        equipped: false,
        in_trade: false,
        reserved: false,
        is_salvage: false,
        structure: 0,
        num_items: 0,
    }];
    let templates = BTreeMap::from([(61, 10)]);
    let proposal = propose_salvage(SalvageRequest {
        actor: ACTOR,
        actor_revision: 1,
        operation_id: [3; 16],
        tool: SalvageTool {
            id: 99,
            owner: ACTOR,
            revision: 1,
            is_ust: true,
            reserved: false,
        },
        skills: SalvageSkills {
            salvaging: 195,
            armor: 0,
            weapon: 0,
            magic_item: 0,
            item: 0,
            augmentations: 0,
        },
        items: &inputs,
        bag_templates: &templates,
        free_slots: 10,
    })
    .unwrap();
    let mut removed = graph(2, 0);
    removed.revision = 2;
    removed.stack = 0;
    removed.place = ItemPlace::Removed;
    let mut shifted = graph(3, 2);
    shifted.revision = 2;
    let mut changes = vec![
        ItemChange {
            before: Some(graph(2, 0)),
            after: removed,
        },
        ItemChange {
            before: Some(graph(3, 1)),
            after: shifted,
        },
    ];
    let generated: Vec<_> = proposal
        .bags
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let mut after = graph(i as u32 + 4, i as u32);
            after.unit_value = b.value;
            after.structure = Some(b.units);
            changes.push(ItemChange {
                before: None,
                after,
            });
            SalvageBagTemplate {
                entity: entity(i as u32 + 4),
                slot: i as u32,
            }
        })
        .collect();
    let inventory = InventoryProposal {
        changes,
        participants: vec![
            (EntityId(ACTOR), 1),
            (EntityId(2), 1),
            (EntityId(3), 1),
            (EntityId(99), 1),
        ],
        actor_burden: 3,
        requires_pickup_motion: false,
    };
    let consumed = [source];
    let others = [sibling];
    let mut wrong_graph = inventory.clone();
    wrong_graph.changes[0].after.stack = 1;
    assert!(
        freeze_salvage(SalvageFreezeInput {
            proposal: &proposal,
            player: &player,
            consumed: &consumed,
            generated: &generated,
            participants: &[ACTOR, 2, 3, 99],
            lease: lease(),
            inventory: &wrong_graph,
            other_items: &others,
        })
        .is_err()
    );
    wrong_graph = inventory.clone();
    wrong_graph.changes.last_mut().unwrap().after.structure = None;
    assert!(
        freeze_salvage(SalvageFreezeInput {
            proposal: &proposal,
            player: &player,
            consumed: &consumed,
            generated: &generated,
            participants: &[ACTOR, 2, 3, 99],
            lease: lease(),
            inventory: &wrong_graph,
            other_items: &others,
        })
        .is_err()
    );
    wrong_graph = inventory.clone();
    wrong_graph.changes.last_mut().unwrap().after.unit_value += 1;
    assert!(
        freeze_salvage(SalvageFreezeInput {
            proposal: &proposal,
            player: &player,
            consumed: &consumed,
            generated: &generated,
            participants: &[ACTOR, 2, 3, 99],
            lease: lease(),
            inventory: &wrong_graph,
            other_items: &others,
        })
        .is_err()
    );
    let operation = freeze_salvage(SalvageFreezeInput {
        proposal: &proposal,
        player: &player,
        consumed: &consumed,
        generated: &generated,
        participants: &[ACTOR, 2, 3, 99],
        lease: lease(),
        inventory: &inventory,
        other_items: &others,
    })
    .unwrap();
    assert_eq!(operation.snapshots.len(), 4);
    let bags: Vec<_> = operation
        .snapshots
        .iter()
        .filter(|s| s.expected_version == 0)
        .map(|s| ItemSaveV5::decode(&s.bytes).unwrap())
        .collect();
    assert_eq!(bags.len(), 2);
    assert!(bags.iter().all(|bag| bag.source_destination.is_none()));
    assert_eq!(
        bags.iter()
            .map(|b| b
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|p| p.id == 92)
                .unwrap()
                .value)
            .sum::<i32>(),
        101
    );
    let shifted = ItemSaveV5::decode(
        &operation
            .snapshots
            .iter()
            .find(|s| s.object_id == 3)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(
        shifted.source_destination,
        others[0].saved.source_destination
    );
    assert_eq!(shifted.enchantments, vec![enchantment()]);
    assert!(matches!(
        shifted.placement,
        ItemPlacementV2::Contained { slot: 2, .. }
    ));
}
