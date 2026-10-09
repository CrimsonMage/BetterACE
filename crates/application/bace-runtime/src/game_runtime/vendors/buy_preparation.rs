//! Cold source-cloned default Shop grants. The simulation still prices and
//! reserves against its live accepted stock before any durable submission.
use super::preparation;
use crate::{
    game_inventory::FrozenInventoryItem, player_entry::PreparedEntryAppearanceAssets,
    region_activation::VerifiedRegionAssets,
};
use bace_inventory::{InventoryItem, ItemPlace};
use bace_persistence::StoredVendorState;
use bace_simulation::{VendorBuyRequest, VendorBuySource};
use bace_storage_codec::{EntitySaveV1, ItemSaveV5, VendorStockSaveV1};
use bace_types::EntityId;
use bace_wire::VendorListing;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) struct GrantPlan {
    stock_id: u32,
    stack: u32,
}

pub(super) struct Prepared {
    pub source: VendorBuySource,
    pub state: StoredVendorState,
    pub listing: VendorListing,
    pub inventory: Vec<InventoryItem>,
    pub fresh: Vec<FrozenInventoryItem>,
    pub appearance: PreparedEntryAppearanceAssets,
    pub operation_id: String,
}

pub(super) fn plan(
    state: &StoredVendorState,
    vendor: EntityId,
    source_revision: u64,
    source_hash: [u8; 32],
    requests: &[VendorBuyRequest],
    max_message_bytes: usize,
) -> Result<(Vec<GrantPlan>, VendorBuySource, VendorListing), String> {
    if vendor.0 == 0 || requests.is_empty() || requests.len() > 1024 {
        return Err("vendor Buy request capacity".into());
    }
    let saved = ItemSaveV5::decode(&state.source.aggregate.bytes).map_err(|e| e.to_string())?;
    let forest = state.forest.as_ref().ok_or("vendor Buy stock not loaded")?;
    let restored = preparation::restore(preparation::RestoreInput {
        vendor,
        vendor_expected_version: state.source.aggregate.persisted_version,
        source: saved.entity.state.clone(),
        source_revision,
        source_hash,
        max_message_bytes,
        forest: forest.clone(),
    })?;
    let marker = VendorStockSaveV1::decode(&forest.marker.bytes).map_err(|e| e.to_string())?;
    if saved.entity.object_id != vendor.0
        || saved.entity.state.weenie_type != 12
        || saved.entity.template_revision != source_revision
    {
        // The restoration adapter independently checks marker history. Keep
        // this read-only projection from accepting a different vendor source.
        return Err("vendor Buy source revision".into());
    }
    let rate = saved
        .entity
        .state
        .properties
        .floats
        .iter()
        .find(|p| p.id == 38)
        .map(|p| p.value)
        .ok_or("vendor SellPrice missing")?;
    if !rate.is_finite() || rate < 0.0 {
        return Err("vendor SellPrice invalid".into());
    }
    let roots: BTreeMap<_, _> = forest
        .items
        .iter()
        .filter(|row| {
            marker
                .defaults
                .iter()
                .any(|entry| entry.root == row.aggregate.object_id)
        })
        .map(|row| (row.aggregate.object_id, row))
        .collect();
    let mut result = Vec::new();
    for request in requests {
        if request.amount <= 0 {
            return Err("vendor Buy nonpositive amount".into());
        }
        let Some(row) = roots.get(&request.stock_id) else {
            continue; // Pinned ACE skips unknown GUIDs after validating amount.
        };
        let item = ItemSaveV5::decode(&row.aggregate.bytes).map_err(|e| e.to_string())?;
        let source = &item.entity.state;
        if matches!(source.weenie_type, 10 | 12 | 15 | 20 | 21 | 61 | 69 | 71)
            || source
                .properties
                .bools
                .iter()
                .any(|p| p.id == 51 && p.value)
            || marker
                .defaults
                .iter()
                .find(|entry| entry.root == request.stock_id)
                .is_none_or(|entry| !entry.child_ids.is_empty() || entry.contribution_units != 0)
        {
            return Err("vendor Buy default leaf only".into());
        }
        let max = int(source, 11).unwrap_or(0);
        if max < 0 {
            return Err("vendor Buy negative maximum stack".into());
        }
        let maximum = if max == 0 { 1 } else { max as u32 };
        let mut amount = u32::try_from(request.amount).map_err(|_| "vendor Buy amount")?;
        while amount != 0 {
            let stack = amount.min(maximum);
            amount -= stack;
            if result.len() >= 1024 {
                return Err("vendor Buy grant capacity".into());
            }
            result.push(GrantPlan {
                stock_id: request.stock_id,
                stack,
            });
        }
    }
    if result.is_empty() {
        return Err("vendor Buy has no supported stock".into());
    }
    Ok((
        result,
        VendorBuySource {
            vendor_expected_version: state.source.aggregate.persisted_version,
            revision: source_revision,
            hash: source_hash,
            sell_rate: Some(rate),
        },
        restored.listing,
    ))
}

pub(super) fn materialize(
    state: StoredVendorState,
    actor: EntityId,
    source: VendorBuySource,
    listing: VendorListing,
    plan: &[GrantPlan],
    ids: &[EntityId],
    assets: &mut VerifiedRegionAssets,
) -> Result<Prepared, String> {
    let (inventory, fresh) = clone_inventory(&state, actor, source, plan, ids)?;
    let sources = fresh
        .iter()
        .map(|item| &item.entity.state)
        .collect::<Vec<_>>();
    let appearance = assets.prepare_entry_appearance(&sources)?;
    Ok(Prepared {
        source,
        state,
        listing,
        inventory,
        fresh,
        appearance,
        operation_id: format!("vendor-buy-{}-{}", actor.0, ids[0].0),
    })
}

fn clone_inventory(
    state: &StoredVendorState,
    actor: EntityId,
    source: VendorBuySource,
    plan: &[GrantPlan],
    ids: &[EntityId],
) -> Result<(Vec<InventoryItem>, Vec<FrozenInventoryItem>), String> {
    if plan.is_empty() || plan.len() != ids.len() || plan.len() > 1024 {
        return Err("vendor Buy grant identity count".into());
    }
    let forest = state.forest.as_ref().ok_or("vendor Buy stock missing")?;
    let marker = VendorStockSaveV1::decode(&forest.marker.bytes).map_err(|e| e.to_string())?;
    let roots: BTreeMap<_, _> = forest
        .items
        .iter()
        .filter(|row| {
            marker
                .defaults
                .iter()
                .any(|entry| entry.root == row.aggregate.object_id)
        })
        .map(|row| (row.aggregate.object_id, row))
        .collect();
    let mut inventory = Vec::with_capacity(ids.len());
    let mut fresh = Vec::with_capacity(ids.len());
    let mut seen = std::collections::BTreeSet::new();
    for (grant, &id) in plan.iter().zip(ids) {
        if !(0x8000_0000..=0xffff_fffe).contains(&id.0) || !seen.insert(id) {
            return Err("vendor Buy grant ID".into());
        }
        let row = roots
            .get(&grant.stock_id)
            .ok_or("vendor Buy source root missing")?;
        let saved = ItemSaveV5::decode(&row.aggregate.bytes).map_err(|e| e.to_string())?;
        let mut state = saved.entity.state.clone();
        if state.weenie_id == 0 || saved.entity.template_revision != source.revision {
            return Err("vendor Buy grant source revision".into());
        }
        let max = int(&state, 11).unwrap_or(0);
        if max < 0 || grant.stack == 0 || grant.stack > max.max(1) as u32 {
            return Err("vendor Buy grant stack".into());
        }
        if max > 1 {
            let unit = int(&state, 15).ok_or("vendor Buy stack unit value")?;
            let value = unit
                .checked_mul(i32::try_from(grant.stack).map_err(|_| "vendor Buy stack range")?)
                .ok_or("vendor Buy stack value")?;
            crate::game_inventory::set(&mut state.properties.ints, 12, grant.stack as i32);
            crate::game_inventory::set(&mut state.properties.ints, 19, value);
        }
        state
            .properties
            .instance_ids
            .retain(|p| ![1, 2, 3, 6].contains(&p.id));
        state.properties.positions.retain(|p| p.id != 1);
        let (item, container) = crate::generator_items::prepare_inventory_item(
            &state,
            id,
            1,
            ItemPlace::Contained {
                container: actor,
                slot: 0,
                equipped: 0,
            },
        )?;
        if container.is_some() || item.is_container || item.stack != grant.stack {
            return Err("vendor Buy grant not a leaf".into());
        }
        inventory.push(item);
        fresh.push(FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: saved.source_destination,
            enchantments: Vec::new(),
            entity: EntitySaveV1 {
                object_id: id.0,
                template_revision: saved.entity.template_revision,
                mutation_revision: 0,
                state,
            },
            placement: None,
            persisted_version: 0,
        });
    }
    Ok((inventory, fresh))
}

fn int(source: &bace_content::WeenieV1, id: u32) -> Option<i32> {
    source
        .properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_default_buy_partitions_exact_stacks_and_rejects_changed_marker() {
        let (ticket, _, _, _, state) = super::super::buy_freezing::tests::fixture();
        let requests = [VendorBuyRequest {
            stock_id: ticket.quote.lines[0].stock_id,
            amount: 11,
        }];
        let (stacks, source, listing) = plan(
            &state,
            ticket.vendor,
            ticket.source_revision,
            ticket.source_hash,
            &requests,
            1 << 20,
        )
        .unwrap();
        assert_eq!(
            stacks.iter().map(|grant| grant.stack).collect::<Vec<_>>(),
            [10, 1]
        );
        assert_eq!(
            source.vendor_expected_version,
            ticket.vendor_expected_version
        );
        assert_eq!(listing.vendor_id, ticket.vendor.0);
        let ids = [EntityId(0x8000_1101), EntityId(0x8000_1102)];
        let (inventory, fresh) =
            clone_inventory(&state, ticket.inventory.actor, source, &stacks, &ids).unwrap();
        assert_eq!(
            inventory.iter().map(|item| item.stack).collect::<Vec<_>>(),
            [10, 1]
        );
        assert_eq!(
            fresh
                .iter()
                .map(|item| int(&item.entity.state, 19))
                .collect::<Vec<_>>(),
            [Some(20), Some(2)]
        );
        assert!(
            fresh
                .iter()
                .all(|item| item.persisted_version == 0 && item.placement.is_none())
        );
        let mut changed = state;
        let forest = changed.forest.as_mut().unwrap();
        let mut marker = VendorStockSaveV1::decode(&forest.marker.bytes).unwrap();
        marker.source_hash[0] ^= 1;
        forest.marker.bytes = marker.encode().unwrap();
        assert!(
            plan(
                &changed,
                ticket.vendor,
                ticket.source_revision,
                ticket.source_hash,
                &requests,
                1 << 20
            )
            .is_err()
        );
    }
}
