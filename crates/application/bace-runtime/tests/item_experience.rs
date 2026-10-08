use bace_content::{Property, WeenieV1};
use bace_persistence::{PlacementOperation, SaveSnapshot};
use bace_runtime::{
    item_experience::prepare_item_experience, item_reward_join::join_reward_item_operations,
};
use bace_types::EntityId;
use std::collections::BTreeMap;
fn source() -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 10,
        class_name: "aetheria".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    }
}
#[test]
fn cold_item_level_gate_and_verified_missing_dat_set() {
    let mut source = source();
    source.properties.strings.push(Property {
        id: 1,
        value: "Aetheria".into(),
    });
    let spells = bace_dat::SpellTable {
        spells: BTreeMap::new(),
        sets: BTreeMap::new(),
    };
    let p = prepare_item_experience(
        EntityId(1),
        EntityId(2),
        3,
        1,
        &source,
        &spells,
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(p.experience.is_none());
    source.properties.int64s = vec![
        Property { id: 4, value: 99 },
        Property { id: 5, value: 100 },
    ];
    source.properties.ints = vec![
        Property { id: 265, value: 1 },
        Property { id: 319, value: 5 },
        Property { id: 320, value: 2 },
    ];
    let p = prepare_item_experience(
        EntityId(1),
        EntityId(2),
        3,
        1,
        &source,
        &spells,
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(p.experience.unwrap().level().unwrap(), 0);
    assert!(p.set.unwrap().tiers.is_empty());
    source.properties.ints[2].value = 4;
    assert!(
        prepare_item_experience(
            EntityId(1),
            EntityId(2),
            3,
            1,
            &source,
            &spells,
            &BTreeMap::new()
        )
        .is_err()
    );
}
fn operation(id: u32) -> PlacementOperation {
    PlacementOperation {
        operation_id: "reward:1:1".into(),
        snapshots: vec![SaveSnapshot {
            object_id: id,
            mutation_revision: 2,
            expected_version: 1,
            bytes: vec![id as u8],
        }],
        participants: vec![id],
        leases: vec![],
        changes: vec![],
        storage_views: vec![],
    }
}
#[test]
fn reward_join_deduplicates_exact_player_and_rolls_back_conflicts() {
    let mut base = operation(1);
    join_reward_item_operations(&mut base, vec![operation(2), operation(1)]).unwrap();
    assert_eq!(base.snapshots.len(), 2);
    assert_eq!(base.participants, vec![1, 2]);
    let before: Vec<_> = base
        .snapshots
        .iter()
        .map(|s| (s.object_id, s.bytes.clone()))
        .collect();
    let mut wrong = operation(1);
    wrong.snapshots[0].bytes = vec![99];
    assert!(join_reward_item_operations(&mut base, vec![operation(3), wrong]).is_err());
    assert_eq!(
        base.snapshots
            .iter()
            .map(|s| (s.object_id, s.bytes.clone()))
            .collect::<Vec<_>>(),
        before
    );
    let mut wrong = operation(3);
    wrong.operation_id = "other".into();
    assert!(join_reward_item_operations(&mut base, vec![wrong]).is_err());
}
#[test]
fn item_xp_freeze_preserves_other_properties_and_rejects_stale_source() {
    use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
    use bace_runtime::{
        game_inventory::{FrozenInventoryItem, InventoryFreezeInput},
        item_experience::freeze_item_experience,
    };
    use bace_storage_codec::{EntitySaveV1, ItemPlacementV2};
    let actor = EntityId(0x50000001);
    let id = EntityId(0x80000001);
    let mut state = source();
    state.properties.strings.push(Property {
        id: 1,
        value: "Aetheria".into(),
    });
    state.properties.int64s = vec![Property { id: 4, value: 9 }, Property { id: 5, value: 10 }];
    state.properties.ints = vec![
        Property { id: 92, value: 6 },
        Property { id: 319, value: 3 },
        Property { id: 320, value: 1 },
    ];
    state.properties.bools.push(Property { id: 1, value: true });
    let before = InventoryItem {
        structure: Some(6),
        id,
        revision: 1,
        template: 10,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: actor,
            slot: 0,
            equipped: 1,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 1,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    let mut after = before.clone();
    after.revision = 2;
    let ticket = bace_simulation::InventoryTicket {
        operation: 1,
        actor,
        proposal: InventoryProposal {
            changes: vec![ItemChange {
                before: Some(before),
                after,
            }],
            participants: vec![(actor, 1), (id, 1)],
            actor_burden: 1,
            requires_pickup_motion: false,
        },
    };
    let xp = bace_character::ItemExperience {
        total: 9,
        base: 10,
        maximum_level: 3,
        style: bace_character::ItemExperienceStyle::Fixed,
        revision: 1,
    };
    let reward = bace_simulation::ItemExperienceReward {
        actor,
        amount: 5,
        changes: vec![(id, xp.propose_xp(5).unwrap())],
        registries: vec![],
        events: vec![],
    };
    let frozen = FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: None,
        enchantments: vec![],
        entity: EntitySaveV1 {
            object_id: id.0,
            template_revision: 3,
            mutation_revision: 1,
            state,
        },
        placement: Some(ItemPlacementV2::Contained {
            container: actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 1,
        }),
        persisted_version: 4,
    };
    let items = vec![frozen];
    let lease = bace_persistence::CharacterLease {
        character_id: actor.0,
        epoch: 1,
        state: bace_persistence::OwnershipState::Online,
    };
    let positions = BTreeMap::new();
    let input = |items| InventoryFreezeInput {
        operation_id: "reward:1:1",
        proposal: &ticket.proposal,
        items,
        other_snapshots: &[],
        leases: std::slice::from_ref(&lease),
        storage_views: &[],
        admitted_positions: &positions,
    };
    let op = freeze_item_experience(input(&items), &reward, &ticket).unwrap();
    let saved = bace_storage_codec::ItemSaveV5::decode(&op.snapshots[0].bytes).unwrap();
    assert_eq!(saved.entity.mutation_revision, 2);
    assert_eq!(
        saved
            .entity
            .state
            .properties
            .int64s
            .iter()
            .find(|p| p.id == 4)
            .unwrap()
            .value,
        14
    );
    assert_eq!(
        saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 92)
            .unwrap()
            .value,
        6
    );
    assert_eq!(
        saved.entity.state.properties.bools,
        items[0].entity.state.properties.bools
    );
    let mut changed_items = items.clone();
    changed_items[0].entity.state.properties.int64s[0].value = 10;
    assert!(freeze_item_experience(input(&changed_items), &reward, &ticket).is_err());
}
