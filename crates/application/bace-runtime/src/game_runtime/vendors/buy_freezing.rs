//! Held default Buy freezer: exact player, coin, grant, vendor and marker CAS.
//! Runtime ingress stays closed until the retained output path is connected.
use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeInput, freeze_inventory, set};
use bace_inventory::ItemPlace;
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, SaveAck, SaveSnapshot, StoredVendorState,
    VendorStockOperation, VendorStockWrite,
};
use bace_simulation::{InventoryReceipt, VendorBuyReceipt, VendorBuyReservation};
use bace_storage_codec::{ItemSaveV5, PlayerSaveV6, VendorStockSaveV1};
use std::collections::{BTreeMap, BTreeSet};

/// `player_before` must be the operation-fenced accepted snapshot composed by
/// `freeze_player_operation_baseline`; `owned` must be its complete current item
/// forest. These inputs are captured only while OnlinePlayerSaveService holds a
/// critical lease. `fresh` contains source-cloned, unplaced complete V5 states.
pub(super) struct BuyFreezeInput<'a> {
    pub ticket: &'a VendorBuyReservation,
    pub player_before: &'a PlayerSaveV6,
    pub player_expected_version: i64,
    pub lease: CharacterLease,
    pub owned: &'a [FrozenInventoryItem],
    pub fresh: &'a [FrozenInventoryItem],
    pub vendor_state: &'a StoredVendorState,
    pub world_epoch: u64,
}

/// The live adapter calls this only after `begin_critical` and the exact
/// `VendorBuy` snapshot outcome. No database write may precede this freeze.
pub(super) fn freeze_captured(
    ticket: &VendorBuyReservation,
    snapshot: &bace_simulation::PlayerReadSnapshot,
    online: &crate::online_player_saves::OnlinePlayerSaveService,
    fresh: &[FrozenInventoryItem],
    vendor_state: &StoredVendorState,
    world_epoch: u64,
    unix_millis: u64,
) -> Result<VendorStockOperation, String> {
    let (baseline, player_expected_version, lease) = online
        .baseline(ticket.inventory.actor.0)
        .ok_or("vendor Buy online player baseline missing")?;
    let player_before = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        snapshot,
        bace_simulation::PlayerSnapshotOperation::VendorBuy(ticket.inventory.operation),
        ticket.actor_revision,
        unix_millis,
    )
    .map_err(|e| e.to_string())?;
    let owned = online.operation_inventory_baselines(snapshot)?;
    freeze(BuyFreezeInput {
        ticket,
        player_before: &player_before,
        player_expected_version,
        lease,
        owned: &owned,
        fresh,
        vendor_state,
        world_epoch,
    })
}

pub(super) fn freeze(input: BuyFreezeInput<'_>) -> Result<VendorStockOperation, String> {
    let ticket = input.ticket;
    let actor = ticket.inventory.actor.0;
    if input.world_epoch == 0
        || input.world_epoch > i64::MAX as u64
        || input.player_expected_version <= 0
        || input.lease.character_id != actor
        || input.lease.state != OwnershipState::Online
        || input.player_before.player.entity.object_id != actor
        || input.player_before.player.account_id == 0
        || input.player_before.player.entity.mutation_revision != ticket.actor_revision
        || ticket.actor_revision == u64::MAX
        || ticket.inventory.proposal.changes.len() > 1024
        || input.fresh.len() > 1024
        || input.owned.len() > 4096
    {
        return Err("vendor Buy player/source bounds".into());
    }
    let source = &input.vendor_state.source;
    let forest = input
        .vendor_state
        .forest
        .as_ref()
        .ok_or("vendor Buy stock marker missing")?;
    let mut vendor = ItemSaveV5::decode(&source.aggregate.bytes).map_err(|e| e.to_string())?;
    let mut marker = VendorStockSaveV1::decode(&forest.marker.bytes).map_err(|e| e.to_string())?;
    if source.aggregate.object_id != ticket.vendor.0
        || source.aggregate.persisted_version != ticket.vendor_expected_version
        || source.aggregate.persisted_version == i64::MAX
        || vendor.entity.object_id != ticket.vendor.0
        || vendor.entity.state.weenie_type != 12
        || vendor.entity.template_revision != ticket.source_revision
        || vendor.entity.mutation_revision == u64::MAX
        || forest.marker.marker_object_id != ticket.marker.0
        || forest.marker.vendor_object_id != ticket.vendor.0
        || forest.marker.persisted_version != ticket.marker_expected_version
        || marker.marker_object_id != ticket.marker.0
        || marker.vendor_object_id != ticket.vendor.0
        || marker.source_revision != ticket.source_revision
        || marker.source_hash != ticket.source_hash
        || marker.stock_revision != ticket.marker_stock_revision
        || !marker.loaded
        || !marker.unique.is_empty()
        || marker
            .defaults
            .iter()
            .any(|entry| entry.contribution_units != 0)
    {
        return Err("vendor Buy source/marker fence".into());
    }
    let grant_changes: Vec<_> = ticket
        .inventory
        .proposal
        .changes
        .iter()
        .filter(|change| change.before.is_none())
        .collect();
    let spent = ticket
        .inventory
        .proposal
        .changes
        .iter()
        .try_fold(0_u32, |total, change| {
            let Some(before) = &change.before else {
                return Some(total);
            };
            (before.template == 273
                && change.after.template == 273
                && before.id == change.after.id
                && change.after.stack < before.stack)
                .then(|| before.stack - change.after.stack)
                .and_then(|amount| total.checked_add(amount))
        });
    if spent != Some(ticket.quote.total_cost) {
        return Err("vendor Buy currency debit mismatch".into());
    }
    let expected_grants: Vec<_> = ticket
        .quote
        .lines
        .iter()
        .flat_map(|line| {
            line.stacks
                .iter()
                .map(|&stack| (line.stock_id, line.template, stack))
        })
        .collect();
    if expected_grants.is_empty()
        || grant_changes.len() != expected_grants.len()
        || input.fresh.len() != expected_grants.len()
    {
        return Err("vendor Buy grant count".into());
    }
    let stock: BTreeMap<_, _> = forest
        .items
        .iter()
        .filter_map(|row| {
            marker
                .defaults
                .iter()
                .any(|entry| entry.root == row.aggregate.object_id)
                .then_some((row.aggregate.object_id, row))
        })
        .collect();
    for (index, (&(stock_id, template, stack), change)) in
        expected_grants.iter().zip(&grant_changes).enumerate()
    {
        let fresh = &input.fresh[index];
        let source = stock
            .get(&stock_id)
            .ok_or("vendor Buy default root missing")?;
        let saved = ItemSaveV5::decode(&source.aggregate.bytes).map_err(|e| e.to_string())?;
        if saved.entity.object_id != stock_id
            || saved.entity.state.weenie_id != template
            || saved.entity.template_revision != ticket.source_revision
            || !saved.previous.previous.enchantments.is_empty()
            || saved.previous.construction.is_some()
            || fresh.entity.object_id != change.after.id.0
            || fresh.entity.state.weenie_id != template
            || fresh.entity.template_revision != saved.entity.template_revision
            || fresh.entity.mutation_revision != 0
            || fresh.persisted_version != 0
            || fresh.placement.is_some()
            || fresh.source_destination != saved.source_destination
            || !fresh.enchantments.is_empty()
            || fresh.construction.is_some()
            || change.after.template != template
            || change.after.stack != stack
            || change.after.revision != 1
            || !matches!(change.after.place, ItemPlace::Contained { equipped: 0, .. })
            || clone_identity(&saved.entity.state) != clone_identity(&fresh.entity.state)
        {
            return Err("vendor Buy grant source identity".into());
        }
    }
    let mut player = input.player_before.clone();
    let cost = i32::try_from(ticket.quote.total_cost).map_err(|_| "vendor Buy cost range")?;
    let old_coin = int(&player.player.entity.state, 20).ok_or("vendor Buy CoinValue missing")?;
    let new_coin = old_coin
        .checked_sub(cost)
        .filter(|value| *value >= 0)
        .ok_or("vendor Buy CoinValue")?;
    let burden = i32::try_from(ticket.inventory.proposal.actor_burden)
        .map_err(|_| "vendor Buy burden range")?;
    set(
        &mut player.player.entity.state.properties.ints,
        20,
        new_coin,
    );
    set(&mut player.player.entity.state.properties.ints, 5, burden);
    player.player.entity.mutation_revision = ticket.actor_revision + 1;
    player.validate().map_err(|e| e.to_string())?;

    let sold = i32::try_from(expected_grants.len()).map_err(|_| "vendor Buy sold count")?;
    let old_sold = int(&vendor.entity.state, 77).unwrap_or(0);
    let old_income = int(&vendor.entity.state, 79).unwrap_or(0);
    if old_sold < 0 || old_income < 0 {
        return Err("vendor Buy negative counter".into());
    }
    set(
        &mut vendor.entity.state.properties.ints,
        77,
        old_sold
            .checked_add(sold)
            .ok_or("vendor Buy sold overflow")?,
    );
    set(
        &mut vendor.entity.state.properties.ints,
        79,
        old_income
            .checked_add(cost)
            .ok_or("vendor Buy income overflow")?,
    );
    vendor.entity.mutation_revision += 1;
    marker.stock_revision = marker
        .stock_revision
        .checked_add(1)
        .ok_or("vendor Buy marker revision overflow")?;
    let marker_version = ticket
        .marker_expected_version
        .checked_add(1)
        .ok_or("vendor Buy marker version overflow")?;
    let extra = [
        SaveSnapshot {
            object_id: actor,
            mutation_revision: player.player.entity.mutation_revision,
            expected_version: input.player_expected_version,
            bytes: player.encode().map_err(|e| e.to_string())?,
        },
        SaveSnapshot {
            object_id: ticket.vendor.0,
            mutation_revision: vendor.entity.mutation_revision,
            expected_version: ticket.vendor_expected_version,
            bytes: vendor.encode().map_err(|e| e.to_string())?,
        },
    ];
    let mut items = input.owned.to_vec();
    items.extend_from_slice(input.fresh);
    let mut inventory = freeze_inventory(InventoryFreezeInput {
        operation_id: &ticket.operation_id,
        proposal: &ticket.inventory.proposal,
        items: &items,
        other_snapshots: &extra,
        leases: &[input.lease],
        storage_views: &[],
        admitted_positions: &BTreeMap::new(),
    })
    .map_err(|e| e.to_string())?;
    let mut participants: BTreeSet<_> = inventory.participants.iter().copied().collect();
    participants.insert(ticket.vendor.0);
    participants.insert(ticket.marker.0);
    participants.extend(marker.defaults.iter().map(|entry| entry.root));
    if participants.len() > 1024 || inventory.snapshots.len() > 1024 {
        return Err("vendor Buy participant capacity".into());
    }
    inventory.participants = participants.into_iter().collect();
    Ok(VendorStockOperation {
        world_epoch: input.world_epoch,
        vendor_expected_version: ticket.vendor_expected_version,
        inventory,
        marker: VendorStockWrite {
            marker_object_id: ticket.marker.0,
            expected_version: ticket.marker_expected_version,
            expected_stock_revision: ticket.marker_stock_revision,
            mutation_revision: u64::try_from(marker_version)
                .map_err(|_| "vendor Buy marker mutation revision")?,
            bytes: marker.encode().map_err(|e| e.to_string())?,
        },
    })
}

/// DB replay validates the operation fingerprint. A new commit must acknowledge
/// every exact snapshot and marker revision before the simulation owner advances.
pub(super) fn receipt(
    ticket: &VendorBuyReservation,
    operation: &VendorStockOperation,
    outcome: OperationOutcome,
) -> Result<(VendorBuyReceipt, Vec<SaveSnapshot>), String> {
    if operation.inventory.operation_id != ticket.operation_id
        || operation.marker.marker_object_id != ticket.marker.0
        || operation.marker.expected_version != ticket.marker_expected_version
    {
        return Err("vendor Buy receipt operation identity".into());
    }
    let marker_version = ticket
        .marker_expected_version
        .checked_add(1)
        .ok_or("vendor Buy marker version")?;
    let expected: BTreeMap<_, _> = operation
        .inventory
        .snapshots
        .iter()
        .map(|row| {
            row.expected_version.checked_add(1).map(|version| {
                (
                    row.object_id,
                    SaveAck {
                        object_id: row.object_id,
                        mutation_revision: row.mutation_revision,
                        persisted_version: version,
                    },
                )
            })
        })
        .collect::<Option<_>>()
        .ok_or("vendor Buy snapshot version overflow")?;
    if expected.len() != operation.inventory.snapshots.len()
        || expected.contains_key(&ticket.marker.0)
        || !expected.contains_key(&ticket.inventory.actor.0)
        || !expected.contains_key(&ticket.vendor.0)
        || ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|change| !expected.contains_key(&change.after.id.0))
    {
        return Err("vendor Buy receipt participants".into());
    }
    if let OperationOutcome::Committed(acks) = outcome {
        let actual: BTreeMap<_, _> = acks.iter().map(|ack| (ack.object_id, ack)).collect();
        if actual.len() != expected.len() + 1
            || acks.len() != actual.len()
            || expected
                .iter()
                .any(|(id, ack)| actual.get(id) != Some(&ack))
            || actual.get(&ticket.marker.0).is_none_or(|ack| {
                ack.persisted_version != marker_version
                    || ack.mutation_revision != operation.marker.mutation_revision
            })
        {
            return Err("vendor Buy durable acknowledgement mismatch".into());
        }
    }
    let mut committed = operation.inventory.snapshots.clone();
    for row in &mut committed {
        row.expected_version += 1;
    }
    Ok((
        VendorBuyReceipt {
            vendor: ticket.vendor,
            marker: ticket.marker,
            operation_id: ticket.operation_id.clone(),
            marker_version,
            marker_stock_revision: ticket
                .marker_stock_revision
                .checked_add(1)
                .ok_or("vendor Buy stock revision")?,
            inventory: InventoryReceipt {
                operation: ticket.inventory.operation,
                revisions: ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .map(|change| (change.after.id, change.after.revision))
                    .collect(),
            },
        },
        committed,
    ))
}

fn int(source: &bace_content::WeenieV1, id: u32) -> Option<i32> {
    source
        .properties
        .ints
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.value)
}

fn clone_identity(source: &bace_content::WeenieV1) -> bace_content::WeenieV1 {
    let mut value = source.clone();
    value
        .properties
        .instance_ids
        .retain(|entry| ![1, 2, 3, 6].contains(&entry.id));
    value.properties.positions.retain(|entry| entry.id != 1);
    value
        .properties
        .ints
        .retain(|entry| ![5, 10, 12, 19, 53].contains(&entry.id));
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Position, Property, WeenieV1};
    use bace_inventory::{InventoryItem, InventoryProposal, ItemChange};
    use bace_persistence::{
        DurableItemPlace, StoredAggregate, StoredVendorSource, StoredVendorStock,
        StoredVendorStockForest, StoredVendorStockItem,
    };
    use bace_simulation::InventoryTicket;
    use bace_simulation::{VendorBuyLine, VendorBuyQuote};
    use bace_storage_codec::{
        EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV3, ItemSaveV4, PlayerSaveV1,
        VendorDefaultStockV1,
    };
    use bace_types::EntityId;

    const ACTOR: u32 = 0x5000_0001;
    const VENDOR: u32 = 0x8000_0101;
    const MARKER: u32 = 0x8000_0102;
    const STOCK: u32 = 0x8000_0103;
    const COIN: u32 = 0x8000_0104;
    const GRANT: u32 = 0x8000_0105;

    fn state(id: u32, kind: u32, ints: &[(u32, i32)]) -> WeenieV1 {
        WeenieV1 {
            schema_version: 1,
            weenie_id: id,
            class_name: format!("buy_fixture_{id}"),
            weenie_type: kind,
            last_modified: None,
            properties: bace_content::SparseProperties {
                ints: ints
                    .iter()
                    .map(|&(id, value)| Property { id, value })
                    .collect(),
                ..Default::default()
            },
        }
    }

    fn position() -> Position {
        Position {
            obj_cell_id: 0x1234_0100,
            position_x: 2.,
            position_y: 3.,
            position_z: 4.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        }
    }

    fn saved(id: u32, state: WeenieV1, place: ItemPlacementV2) -> ItemSaveV5 {
        let v1 = EntitySaveV1 {
            object_id: id,
            template_revision: 7,
            mutation_revision: 1,
            state,
        };
        let v2 = ItemSaveV2::migrate_v1(v1, place).unwrap();
        let v3 = ItemSaveV3::migrate_v2(v2).unwrap();
        let v4 = ItemSaveV4::migrate_v3(v3).unwrap();
        ItemSaveV5 {
            previous: v4,
            source_destination: (id == STOCK || id == GRANT).then_some(4),
        }
    }

    fn item(id: u32, template: u32, stack: u32, revision: u64) -> InventoryItem {
        InventoryItem {
            id: EntityId(id),
            revision,
            template,
            stack_key: u64::from(template),
            place: ItemPlace::Contained {
                container: EntityId(ACTOR),
                slot: 0,
                equipped: 0,
            },
            stack,
            maximum_stack: 10,
            unit_burden: 0,
            unit_value: if template == 273 { 1 } else { 2 },
            structure: None,
            pack_slot: false,
            is_container: false,
            attuned: false,
            trade_reserved: false,
            active_pet: false,
            unique: false,
            quest_allowed: true,
            valid_wield: 0,
            incompatible_wield: 0,
            wield_requirements_met: false,
        }
    }

    fn fixture() -> (
        VendorBuyReservation,
        PlayerSaveV6,
        Vec<FrozenInventoryItem>,
        Vec<FrozenInventoryItem>,
        StoredVendorState,
    ) {
        let player = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: ACTOR,
                template_revision: 7,
                mutation_revision: 1,
                state: state(1, 10, &[(5, 0), (20, 10)]),
            },
            account_id: 1,
            name: "Buyer".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap();
        let coin = saved(
            COIN,
            state(273, 14, &[(5, 0), (12, 10), (19, 10)]),
            ItemPlacementV2::Contained {
                container: ACTOR,
                slot: 0,
                pack_slot: false,
                equipped: 0,
            },
        );
        let stock = saved(
            STOCK,
            state(100, 1, &[(5, 0), (11, 10), (12, 1), (15, 2), (19, 2)]),
            ItemPlacementV2::Contained {
                container: VENDOR,
                slot: 0,
                pack_slot: false,
                equipped: 0,
            },
        );
        let fresh = FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: Some(4),
            enchantments: vec![],
            entity: EntitySaveV1 {
                object_id: GRANT,
                template_revision: 7,
                mutation_revision: 0,
                state: stock.entity.state.clone(),
            },
            placement: None,
            persisted_version: 0,
        };
        let marker = VendorStockSaveV1 {
            marker_object_id: MARKER,
            vendor_object_id: VENDOR,
            source_revision: 7,
            source_hash: [9; 32],
            loaded: true,
            stock_revision: 2,
            defaults: vec![VendorDefaultStockV1 {
                root: STOCK,
                display_quantity: -1,
                contribution_units: 0,
                child_ids: vec![],
            }],
            unique: vec![],
        };
        let vendor = saved(
            VENDOR,
            state(50, 12, &[(77, 0), (79, 0)]),
            ItemPlacementV2::World(position()),
        );
        let state = StoredVendorState {
            source: StoredVendorSource {
                aggregate: StoredAggregate {
                    object_id: VENDOR,
                    persisted_version: 1,
                    bytes: vendor.encode().unwrap(),
                },
                cell: position().obj_cell_id,
            },
            forest: Some(StoredVendorStockForest {
                marker: StoredVendorStock {
                    vendor_object_id: VENDOR,
                    marker_object_id: MARKER,
                    persisted_version: 1,
                    bytes: marker.encode().unwrap(),
                },
                items: vec![StoredVendorStockItem {
                    aggregate: StoredAggregate {
                        object_id: STOCK,
                        persisted_version: 1,
                        bytes: stock.encode().unwrap(),
                    },
                    placement: DurableItemPlace::Contained {
                        container: VENDOR,
                        slot: 0,
                        pack_slot: false,
                        equipped: 0,
                    },
                }],
            }),
        };
        let mut paid = item(COIN, 273, 8, 2);
        paid.unit_value = 1;
        let ticket = VendorBuyReservation {
            vendor: EntityId(VENDOR),
            marker: EntityId(MARKER),
            actor_revision: 1,
            marker_expected_version: 1,
            marker_stock_revision: 2,
            vendor_expected_version: 1,
            source_revision: 7,
            source_hash: [9; 32],
            operation_id: "vendor-buy-fixture".into(),
            quote: VendorBuyQuote {
                stock_revision: 2,
                lines: vec![VendorBuyLine {
                    stock_id: STOCK,
                    template: 100,
                    stacks: vec![1],
                    cost: 2,
                }],
                total_cost: 2,
            },
            inventory: InventoryTicket {
                operation: 1,
                actor: EntityId(ACTOR),
                proposal: InventoryProposal {
                    changes: vec![
                        ItemChange {
                            before: Some(item(COIN, 273, 10, 1)),
                            after: paid,
                        },
                        ItemChange {
                            before: None,
                            after: item(GRANT, 100, 1, 1),
                        },
                    ],
                    participants: vec![(EntityId(ACTOR), 1), (EntityId(COIN), 1)],
                    actor_burden: 0,
                    requires_pickup_motion: false,
                },
            },
        };
        let owned = vec![FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            enchantments: vec![],
            entity: coin.entity.clone(),
            placement: Some(coin.placement.clone()),
            persisted_version: 1,
        }];
        (ticket, player, owned, vec![fresh], state)
    }

    #[test]
    fn joined_freeze_preserves_vendor_pose_and_requires_every_exact_ack() {
        let (ticket, player, owned, fresh, state) = fixture();
        let input = || BuyFreezeInput {
            ticket: &ticket,
            player_before: &player,
            player_expected_version: 1,
            lease: CharacterLease {
                character_id: ACTOR,
                epoch: 1,
                state: OwnershipState::Online,
            },
            owned: &owned,
            fresh: &fresh,
            vendor_state: &state,
            world_epoch: 2,
        };
        let operation = freeze(input()).unwrap();
        assert_eq!(operation.inventory.snapshots.len(), 4);
        let vendor = ItemSaveV5::decode(
            &operation
                .inventory
                .snapshots
                .iter()
                .find(|row| row.object_id == VENDOR)
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!(vendor.placement, ItemPlacementV2::World(position()));
        assert_eq!(int(&vendor.entity.state, 77), Some(1));
        assert_eq!(int(&vendor.entity.state, 79), Some(2));
        assert_eq!(
            VendorStockSaveV1::decode(&operation.marker.bytes)
                .unwrap()
                .stock_revision,
            3
        );
        let mut acks: Vec<_> = operation
            .inventory
            .snapshots
            .iter()
            .map(|row| SaveAck {
                object_id: row.object_id,
                mutation_revision: row.mutation_revision,
                persisted_version: row.expected_version + 1,
            })
            .collect();
        acks.push(SaveAck {
            object_id: MARKER,
            mutation_revision: operation.marker.mutation_revision,
            persisted_version: 2,
        });
        let (buy_receipt, committed) = receipt(
            &ticket,
            &operation,
            OperationOutcome::Committed(acks.clone()),
        )
        .unwrap();
        assert_eq!(buy_receipt.marker_stock_revision, 3);
        assert_eq!(buy_receipt.inventory.revisions.len(), 2);
        assert_eq!(committed.len(), 4);
        assert_eq!(
            committed
                .iter()
                .find(|row| row.object_id == ACTOR)
                .unwrap()
                .expected_version,
            2
        );
        acks.pop();
        assert!(receipt(&ticket, &operation, OperationOutcome::Committed(acks)).is_err());
        assert!(receipt(&ticket, &operation, OperationOutcome::AlreadyCommitted).is_ok());
    }

    #[test]
    fn stale_marker_or_altered_fresh_quality_never_freezes() {
        let (ticket, player, owned, mut fresh, mut state) = fixture();
        set(&mut fresh[0].entity.state.properties.ints, 999, 42);
        let input = BuyFreezeInput {
            ticket: &ticket,
            player_before: &player,
            player_expected_version: 1,
            lease: CharacterLease {
                character_id: ACTOR,
                epoch: 1,
                state: OwnershipState::Online,
            },
            owned: &owned,
            fresh: &fresh,
            vendor_state: &state,
            world_epoch: 2,
        };
        assert!(freeze(input).is_err());
        fresh[0]
            .entity
            .state
            .properties
            .ints
            .retain(|entry| entry.id != 999);
        state.forest.as_mut().unwrap().marker.persisted_version = 2;
        assert!(
            freeze(BuyFreezeInput {
                ticket: &ticket,
                player_before: &player,
                player_expected_version: 1,
                lease: CharacterLease {
                    character_id: ACTOR,
                    epoch: 1,
                    state: OwnershipState::Online,
                },
                owned: &owned,
                fresh: &fresh,
                vendor_state: &state,
                world_epoch: 2,
            })
            .is_err()
        );
    }
}
