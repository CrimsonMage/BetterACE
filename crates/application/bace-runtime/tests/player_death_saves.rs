use bace_content::{Position, Property, SecondaryAttribute, WeenieV1};
use bace_entity::{EntityVital, VitalMutation};
use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
use bace_magic::{EnchantmentEntry, EnchantmentMetadata, EnchantmentSpec, MagicSchool};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::{game_inventory::FrozenInventoryItem, player_death_saves::*};
use bace_simulation::{InventoryTicket, PlayerDeathState, PlayerDeathTicket, PlayerNoCorpsePlan};
use bace_storage_codec::{
    CorpseSaveV5, EntitySaveV1, ItemPlacementV2, ItemSaveV5, PlayerSaveV1, PlayerSaveV6,
};
use bace_types::{CellId, EntityId};
use std::collections::BTreeMap;
const PLAYER: EntityId = EntityId(0x50000001);
const CORPSE: EntityId = EntityId(0x80000001);
const ITEM: EntityId = EntityId(0x80000002);
fn source(id: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("death_fixture_{id}"),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    }
}
fn position() -> Position {
    Position {
        obj_cell_id: 0x01010001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    }
}
fn item(id: EntityId, place: ItemPlace, container: bool, revision: u64) -> InventoryItem {
    InventoryItem {
        id,
        revision,
        template: 100,
        stack_key: 1,
        place,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: container,
        is_container: container,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
        structure: None,
    }
}
fn entry(spell: u32) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster: PLAYER.0,
        school: MagicSchool::Life,
        spec: EnchantmentSpec {
            category: if spell == 666 { 204 } else { 1 },
            power: 1,
            duration: if spell == 666 { -1. } else { 120. },
            layer: 1,
            stat_type: 0,
            stat_key: 0,
            value: 0.95,
            beneficial: true,
            set_id: None,
        },
        start_time: 0.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: EnchantmentMetadata::default(),
    }
}
fn fixture() -> (
    PlayerSaveV6,
    PlayerDeathTicket,
    Vec<FrozenInventoryItem>,
    BTreeMap<u32, Position>,
) {
    let mut state = source(1);
    state.properties.secondary_attributes = [1, 3, 5]
        .into_iter()
        .map(|id| Property {
            id,
            value: SecondaryAttribute {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: if id == 1 { 0 } else { 100 },
            },
        })
        .collect();
    let mut player = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: PLAYER.0,
            template_revision: 1,
            mutation_revision: 4,
            state,
        },
        account_id: 1,
        name: "Death Fixture".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    player.enchantments =
        vec![bace_runtime::enchantment_saves::freeze_enchantment(&entry(100)).unwrap()];
    let before_item = item(
        ITEM,
        ItemPlace::Contained {
            container: PLAYER,
            slot: 0,
            equipped: 0,
        },
        false,
        2,
    );
    let after_item = item(
        ITEM,
        ItemPlace::Contained {
            container: CORPSE,
            slot: 0,
            equipped: 0,
        },
        false,
        3,
    );
    let corpse = item(CORPSE, ItemPlace::World, true, 0);
    let changes = vec![
        ItemChange {
            before: Some(before_item),
            after: after_item,
        },
        ItemChange {
            before: None,
            after: corpse,
        },
    ];
    let before = PlayerDeathState::default();
    let mut after = before.clone();
    after.num_deaths = 1;
    after.protection_elapsed = Some(0.);
    let ticket = PlayerDeathTicket {
        operation: 8,
        actor: PLAYER,
        killer: None,
        kind: bace_interactions::PlayerDeathKind::Ordinary,
        olthoi: None,
        before_revision: 4,
        after_revision: 5,
        before,
        after,
        purge_bad: false,
        inventory: InventoryTicket {
            operation: 9,
            actor: PLAYER,
            proposal: InventoryProposal {
                changes,
                participants: vec![(PLAYER, 4), (ITEM, 2)],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
        inventory_transcript: None,
        corpse: CORPSE,
        no_corpse: None,
        corpse_items: vec![ITEM],
        corpse_decay_seconds: 3600,
        registry_before_revision: 1,
        registry_after_revision: 3,
        before_enchantments: vec![entry(100)],
        enchantments: vec![entry(666)],
        destination: bace_world::WorldTeleport {
            actor: PLAYER,
            expected_epoch: 1,
            destination: CellId(0x02020001),
            position: bace_geometry::Vec3::new(5., 6., 7.),
            heading: 0.,
        },
        vitals: [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .enumerate()
            .map(|(i, vital)| VitalMutation {
                actor: PLAYER,
                vital,
                before: if i == 0 { 0 } else { 100 },
                after: 71,
            })
            .collect(),
        post_death_maxima: [95; 3],
        animation_ticks: 30,
    };
    let mut corpse_source = source(100);
    corpse_source.weenie_type = 14;
    let items = vec![
        FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            entity: EntitySaveV1 {
                object_id: ITEM.0,
                template_revision: 1,
                mutation_revision: 2,
                state: source(100),
            },
            placement: Some(ItemPlacementV2::Contained {
                container: PLAYER.0,
                slot: 0,
                equipped: 0,
                pack_slot: false,
            }),
            enchantments: vec![],
            persisted_version: 1,
        },
        FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            entity: EntitySaveV1 {
                object_id: CORPSE.0,
                template_revision: 1,
                mutation_revision: 0,
                state: corpse_source,
            },
            placement: None,
            enchantments: vec![],
            persisted_version: 0,
        },
    ];
    (
        player,
        ticket,
        items,
        BTreeMap::from([(CORPSE.0, position())]),
    )
}

#[test]
fn no_corpse_world_root_and_player_checkpoint_share_one_operation() {
    let (player, mut ticket, mut items, _) = fixture();
    items.pop();
    items[0].source_destination = Some(8);
    items[0].entity.state.properties.strings.push(Property {
        id: 33,
        value: "quest source".into(),
    });
    let before = ticket.inventory.proposal.changes[0].before.clone();
    let mut after = before.clone().unwrap();
    after.place = ItemPlace::World;
    after.revision += 1;
    ticket.inventory.proposal.changes = vec![ItemChange { before, after }];
    ticket.corpse = EntityId(0);
    ticket.corpse_items.clear();
    ticket.corpse_decay_seconds = 0;
    ticket.no_corpse = Some(PlayerNoCorpsePlan {
        world_roots: vec![ITEM],
        descendants: vec![],
        accepted_position: position(),
    });
    let frozen = freeze_player_death(PlayerDeathFreezeInput {
        epoch: 7,
        ticket: &ticket,
        player: &player,
        persisted_version: 2,
        lease: CharacterLease {
            character_id: PLAYER.0,
            epoch: 1,
            state: OwnershipState::Online,
        },
        items: &items,
        positions: &BTreeMap::from([(ITEM.0, position())]),
        unix_seconds: 100,
    })
    .unwrap();
    assert_eq!(frozen.expires_at, 0);
    assert_eq!(frozen.operation.snapshots.len(), 2);
    let saved = frozen
        .operation
        .snapshots
        .iter()
        .find(|row| row.object_id == ITEM.0)
        .unwrap();
    let saved = ItemSaveV5::decode(&saved.bytes).unwrap();
    assert_eq!(saved.source_destination, Some(8));
    assert_eq!(saved.placement, ItemPlacementV2::World(position()));
    assert_eq!(
        saved
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 6)
            .map(|p| p.value),
        Some(PLAYER.0)
    );
}

#[test]
fn no_corpse_zero_drop_still_commits_player_checkpoint() {
    let (player, mut ticket, _, _) = fixture();
    ticket.inventory.proposal.changes.clear();
    ticket.inventory.proposal.participants = vec![(PLAYER, 4)];
    ticket.corpse = EntityId(0);
    ticket.corpse_items.clear();
    ticket.corpse_decay_seconds = 0;
    ticket.no_corpse = Some(PlayerNoCorpsePlan {
        world_roots: vec![],
        descendants: vec![],
        accepted_position: position(),
    });
    let frozen = freeze_player_death(PlayerDeathFreezeInput {
        epoch: 7,
        ticket: &ticket,
        player: &player,
        persisted_version: 2,
        lease: CharacterLease {
            character_id: PLAYER.0,
            epoch: 1,
            state: OwnershipState::Online,
        },
        items: &[],
        positions: &BTreeMap::new(),
        unix_seconds: 100,
    })
    .unwrap();
    assert_eq!(frozen.expires_at, 0);
    assert_eq!(frozen.operation.snapshots.len(), 1);
    assert_eq!(frozen.operation.snapshots[0].object_id, PLAYER.0);
}
fn freeze(
    player: &PlayerSaveV6,
    ticket: &PlayerDeathTicket,
    items: &[FrozenInventoryItem],
    positions: &BTreeMap<u32, Position>,
) -> Result<FrozenPlayerDeath, String> {
    freeze_player_death(PlayerDeathFreezeInput {
        epoch: 2,
        ticket,
        player,
        persisted_version: 10,
        lease: CharacterLease {
            character_id: PLAYER.0,
            epoch: 3,
            state: OwnershipState::Online,
        },
        items,
        positions,
        unix_seconds: 1000,
    })
}
#[test]
fn one_operation_checkpoints_alive_player_purged_registry_corpse_and_loss() {
    let (player, ticket, items, positions) = fixture();
    let f = freeze(&player, &ticket, &items, &positions).unwrap();
    assert_eq!(f.operation.snapshots.len(), 3);
    assert_eq!(f.operation.leases.len(), 1);
    let corpse = CorpseSaveV5::decode(
        &f.operation
            .snapshots
            .iter()
            .find(|s| s.object_id == CORPSE.0)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(corpse.previous.corpse.expires_at, 4600);
    assert_eq!(corpse.previous.corpse.owner, Some(PLAYER.0));
    assert_eq!(corpse.previous.corpse.death_operation, "player-death:2:8");
    let lost = ItemSaveV5::decode(
        &f.operation
            .snapshots
            .iter()
            .find(|s| s.object_id == ITEM.0)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert!(
        matches!(lost.placement,ItemPlacementV2::Contained{container,..} if container==CORPSE.0)
    );
    assert!(
        f.player
            .player
            .entity
            .state
            .properties
            .secondary_attributes
            .iter()
            .all(|v| v.value.current_level == 71)
    );
    assert_eq!(f.player.enchantments.len(), 1);
    assert_eq!(
        f.player.enchantments[0],
        bace_runtime::enchantment_saves::freeze_enchantment(&entry(666)).unwrap()
    );
    assert_eq!(
        bace_runtime::player_death_state::restore_player_death_state(&f.player)
            .unwrap()
            .num_deaths,
        1
    );
    assert_eq!(f.receipt.inventory.revisions.len(), 2);
}

#[test]
fn olthoi_nested_treasure_is_one_durable_corpse_forest_without_victim_loss() {
    let (player, mut ticket, mut items, positions) = fixture();
    let root = EntityId(0x80000003);
    let child = EntityId(0x80000004);
    let younger_child = EntityId(0x80000005);
    ticket.olthoi = Some(bace_simulation::OlthoiDeathKind::Treasure);
    ticket.corpse_items = vec![root];
    ticket.inventory.proposal.changes = vec![
        ItemChange {
            before: None,
            after: item(
                root,
                ItemPlace::Contained {
                    container: CORPSE,
                    slot: 0,
                    equipped: 0,
                },
                true,
                0,
            ),
        },
        ItemChange {
            before: None,
            after: item(
                child,
                ItemPlace::Contained {
                    container: root,
                    slot: 1,
                    equipped: 0,
                },
                false,
                0,
            ),
        },
        ItemChange {
            before: None,
            after: item(
                younger_child,
                ItemPlace::Contained {
                    container: root,
                    slot: 0,
                    equipped: 0,
                },
                false,
                0,
            ),
        },
        ticket.inventory.proposal.changes.pop().unwrap(),
    ];
    ticket.inventory.proposal.participants = vec![(PLAYER, 4)];
    let mut root_source = source(100);
    root_source.weenie_type = 20;
    let mut child_source = source(100);
    child_source.properties.instance_ids.push(Property {
        id: 6,
        value: root.0,
    });
    for (id, state) in [
        (root, root_source),
        (child, child_source.clone()),
        (younger_child, child_source),
    ] {
        items.push(FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            entity: EntitySaveV1 {
                object_id: id.0,
                template_revision: 1,
                mutation_revision: 0,
                state,
            },
            placement: None,
            enchantments: vec![],
            persisted_version: 0,
        });
    }
    let frozen = freeze(&player, &ticket, &items, &positions).unwrap();
    assert_eq!(frozen.operation.snapshots.len(), 5);
    assert!(
        !frozen
            .operation
            .snapshots
            .iter()
            .any(|row| row.object_id == ITEM.0)
    );
    for (id, parent, slot) in [
        (root, CORPSE, 0),
        (child, root, 1),
        (younger_child, root, 0),
    ] {
        let row = frozen
            .operation
            .snapshots
            .iter()
            .find(|row| row.object_id == id.0)
            .unwrap();
        let saved = ItemSaveV5::decode(&row.bytes).unwrap();
        assert!(
            matches!(saved.placement,ItemPlacementV2::Contained{container,slot:stored,..} if container==parent.0 && stored==slot)
        );
        if id != root {
            assert_eq!(
                saved
                    .entity
                    .state
                    .properties
                    .instance_ids
                    .iter()
                    .find(|property| property.id == 6)
                    .map(|property| property.value),
                Some(root.0)
            );
        }
    }
}
#[test]
fn stale_registry_wrong_corpse_membership_and_dead_checkpoint_are_rejected() {
    let (mut player, mut ticket, items, positions) = fixture();
    let original = player.enchantments.clone();
    player.enchantments.clear();
    assert!(
        freeze(&player, &ticket, &items, &positions)
            .err()
            .expect("stale registry rejected")
            .contains("registry")
    );
    player.enchantments = original;
    ticket.corpse_items.clear();
    assert!(freeze(&player, &ticket, &items, &positions).is_err());
    ticket.corpse_items.push(ITEM);
    ticket.vitals[0].after = 0;
    assert!(freeze(&player, &ticket, &items, &positions).is_err());
}
#[test]
fn expiry_conversion_accounts_for_commit_delay_and_overflow() {
    assert_eq!(corpse_expiry_tick(100, 95, 300).unwrap(), 450);
    assert_eq!(corpse_expiry_tick(100, 101, 300).unwrap(), 300);
    assert!(corpse_expiry_tick(i64::MAX, 0, u64::MAX).is_err());
}
#[test]
fn monster_corpse_expiry_keeps_schema_and_metadata_while_tombstoning_all_contents() {
    let (player, death, items, positions) = fixture();
    let f = freeze(&player, &death, &items, &positions).unwrap();
    let mut corpse = CorpseSaveV5::decode(
        &f.operation
            .snapshots
            .iter()
            .find(|s| s.object_id == CORPSE.0)
            .unwrap()
            .bytes,
    )
    .unwrap();
    corpse.source = Some(0x80000050);
    corpse.access.victim = corpse.source;
    corpse.access.is_monster = true;
    let frozen_items: Vec<_> = f
        .operation
        .snapshots
        .iter()
        .filter(|s| s.object_id != PLAYER.0)
        .map(|s| {
            if s.object_id == CORPSE.0 {
                FrozenInventoryItem {
                    corpse: None,
                    construction: None,
                    source_destination: None,
                    entity: corpse.corpse.entity.clone(),
                    placement: Some(corpse.placement.clone()),
                    enchantments: corpse.enchantments.clone(),
                    persisted_version: 1,
                }
            } else {
                let item = ItemSaveV5::decode(&s.bytes).unwrap();
                FrozenInventoryItem {
                    corpse: None,
                    construction: item.construction.clone(),
                    source_destination: item.source_destination,
                    entity: item.entity.clone(),
                    placement: Some(item.placement.clone()),
                    enchantments: item.enchantments.clone(),
                    persisted_version: 2,
                }
            }
        })
        .collect();
    let changes: Vec<_> = death
        .inventory
        .proposal
        .changes
        .iter()
        .map(|c| {
            let before = c.after.clone();
            let mut after = before.clone();
            after.place = ItemPlace::Removed;
            after.revision += 1;
            ItemChange {
                before: Some(before),
                after,
            }
        })
        .collect();
    let expiry = bace_simulation::CorpseExpiryTicket {
        corpse: CORPSE,
        death_operation: 8,
        expires_tick: 108000,
        inventory: InventoryTicket {
            operation: 10,
            actor: CORPSE,
            proposal: InventoryProposal {
                participants: changes
                    .iter()
                    .map(|c| (c.after.id, c.before.as_ref().unwrap().revision))
                    .collect(),
                changes,
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
        transient: vec![],
        spill: None,
        enchantments: BTreeMap::new(),
    };
    let mut player_corpse = corpse.clone();
    player_corpse.source = Some(PLAYER.0);
    player_corpse.access.victim = player_corpse.source;
    player_corpse.access.is_monster = false;
    assert!(
        bace_runtime::corpse_expiry_saves::freeze_corpse_expiry(
            bace_runtime::corpse_expiry_saves::CorpseExpiryFreezeInput {
                spill_positions: &BTreeMap::new(),
                world_epoch: 2,
                ticket: &expiry,
                corpse: &player_corpse,
                items: &frozen_items,
                unix_seconds: 4600,
            },
        )
        .is_err(),
        "player contents require geometry-backed world spills, never tombstones"
    );
    let pending = bace_runtime::corpse_expiry_saves::freeze_corpse_expiry(
        bace_runtime::corpse_expiry_saves::CorpseExpiryFreezeInput {
            spill_positions: &BTreeMap::new(),
            world_epoch: 2,
            ticket: &expiry,
            corpse: &corpse,
            items: &frozen_items,
            unix_seconds: 4600,
        },
    )
    .unwrap();
    let snapshots = &pending.save.operation().snapshots;
    assert_eq!(snapshots.len(), 2);
    let tombstone = CorpseSaveV5::decode(
        &snapshots
            .iter()
            .find(|s| s.object_id == CORPSE.0)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(tombstone.placement, ItemPlacementV2::Removed);
    assert_eq!(
        tombstone.corpse.death_operation,
        corpse.corpse.death_operation
    );
    assert_eq!(tombstone.corpse.expires_at, 4600);
    let mut spill = expiry.clone();
    let location = bace_gameplay_api::GeneratorLocation {
        cell: position().obj_cell_id,
        origin: [1., 2., 3.],
        rotation: [0., 0., 0., 1.],
    };
    spill.spill = Some(bace_simulation::CorpseSpillIntent {
        roots: vec![ITEM],
        position: location,
    });
    spill
        .inventory
        .proposal
        .changes
        .iter_mut()
        .find(|c| c.after.id == ITEM)
        .unwrap()
        .after
        .place = ItemPlace::World;
    let poses = BTreeMap::from([(
        ITEM.0,
        bace_runtime::corpse_expiry_saves::corpse_spill_position(location, None).unwrap(),
    )]);
    let mut spill_items = frozen_items.clone();
    let source = spill_items
        .iter_mut()
        .find(|i| i.entity.object_id == ITEM.0)
        .unwrap();
    source.persisted_version = 0;
    source.entity.state.properties.instance_ids.push(Property {
        id: 6,
        value: PLAYER.0,
    });
    spill.transient = vec![ITEM];
    let spilled = bace_runtime::corpse_expiry_saves::freeze_corpse_expiry(
        bace_runtime::corpse_expiry_saves::CorpseExpiryFreezeInput {
            world_epoch: 2,
            ticket: &spill,
            corpse: &player_corpse,
            items: &spill_items,
            unix_seconds: 4600,
            spill_positions: &poses,
        },
    )
    .unwrap();
    let saved = ItemSaveV5::decode(
        &spilled
            .save
            .operation()
            .snapshots
            .iter()
            .find(|s| s.object_id == ITEM.0)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(
        saved.placement,
        ItemPlacementV2::World(poses[&ITEM.0].clone())
    );
    assert_eq!(
        saved
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 6)
            .unwrap()
            .value,
        PLAYER.0
    );
    assert_eq!(
        saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 65)
            .unwrap()
            .value,
        101
    );
    assert!(
        spilled
            .save
            .operation()
            .changes
            .iter()
            .any(|c| c.item == ITEM.0
                && !matches!(c.destination, bace_persistence::DurableItemPlace::Removed))
    );
    // A never-persisted generated corpse tree uses the same immutable archive,
    // but every tombstone must enter the world-epoch journal as a first write.
    let mut transient = expiry.clone();
    transient.transient = transient
        .inventory
        .proposal
        .changes
        .iter()
        .map(|c| c.after.id)
        .collect();
    let mut archived = frozen_items.clone();
    for source in &mut archived {
        source.persisted_version = 0;
    }
    let first = bace_runtime::corpse_expiry_saves::freeze_corpse_expiry(
        bace_runtime::corpse_expiry_saves::CorpseExpiryFreezeInput {
            spill_positions: &BTreeMap::new(),
            world_epoch: 2,
            ticket: &transient,
            corpse: &corpse,
            items: &archived,
            unix_seconds: 4600,
        },
    )
    .unwrap();
    assert!(
        first
            .save
            .operation()
            .snapshots
            .iter()
            .all(|s| s.expected_version == 0)
    );
    assert!(
        first
            .save
            .operation()
            .changes
            .iter()
            .all(|c| c.expected.is_none())
    );
    assert_eq!(first.receipt, pending.receipt);
    archived[0].persisted_version = 1;
    assert!(
        bace_runtime::corpse_expiry_saves::freeze_corpse_expiry(
            bace_runtime::corpse_expiry_saves::CorpseExpiryFreezeInput {
                spill_positions: &BTreeMap::new(),
                world_epoch: 2,
                ticket: &transient,
                corpse: &corpse,
                items: &archived,
                unix_seconds: 4600,
            },
        )
        .is_err()
    );
    assert!(
        bace_runtime::corpse_expiry_saves::freeze_corpse_expiry(
            bace_runtime::corpse_expiry_saves::CorpseExpiryFreezeInput {
                spill_positions: &BTreeMap::new(),
                world_epoch: 2,
                ticket: &expiry,
                corpse: &corpse,
                items: &frozen_items,
                unix_seconds: 4599
            }
        )
        .is_err()
    );
}

#[test]
fn original_corpse_spill_positions_use_float_precision() {
    let mut count = 0;
    for line in include_str!("fixtures/corpse_spill.trace")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let values: Vec<_> = line.split('|').collect();
        let location = bace_gameplay_api::GeneratorLocation {
            cell: 0x01010001,
            origin: [1., 2., values[0].parse().unwrap()],
            rotation: [0., 0., 0., 1.],
        };
        let scale = (values[1] != "-").then(|| values[1].parse::<f64>().unwrap());
        let result =
            bace_runtime::corpse_expiry_saves::corpse_spill_position(location, scale).unwrap();
        assert_eq!(
            result.position_z.to_bits(),
            u32::from_str_radix(values[2], 16).unwrap(),
            "{line}"
        );
        assert_eq!(values[3], "101");
        assert_eq!(values[4], "1");
        count += 1;
    }
    assert_eq!(count, 24);
}
