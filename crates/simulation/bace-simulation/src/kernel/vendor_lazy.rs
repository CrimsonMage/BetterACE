//! First-Use Shop stock is prepared cold, reserved under one vendor owner, and
//! published only after the exact marker plus item forest durable receipt.
use super::*;
use crate::vendor_commands::{
    VendorAction, VendorCommand, VendorCommandError, VendorDecision, VendorOutcome,
};
use crate::vendor_trees::{PreparedVendorLazyStock, VendorLazyStockReceipt, VendorLazyStockTicket};
use crate::{GeneratorServiceError as G, VendorContents};
use bace_economy::{VendorStock, VendorStockItem};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) struct PendingVendorLazy {
    pub ticket: VendorLazyStockTicket,
    stock: VendorStock,
    items: BTreeMap<u32, Arc<bace_content::WeenieV1>>,
    contents: BTreeMap<u32, VendorContents>,
    display_quantities: BTreeMap<u32, i32>,
}

impl Kernel {
    pub fn pop_vendor_outcome(&mut self) -> Option<VendorOutcome> {
        self.vendor_outcomes.pop_front()
    }

    pub fn vendor_outcomes_pending(&self) -> bool {
        !self.vendor_outcomes.is_empty()
    }

    pub fn vendor_command(&mut self, command: VendorCommand) -> VendorOutcome {
        if !command.valid_bounds() {
            let error = match command.action {
                VendorAction::ReserveDefaultBuy { .. }
                | VendorAction::RejectBuy(_)
                | VendorAction::ConfirmBuy { .. } => {
                    VendorCommandError::Buy(crate::VendorBuyError::Invalid)
                }
                _ => VendorCommandError::Lazy(G::Invalid),
            };
            return VendorOutcome {
                correlation: command.correlation,
                result: Err(error),
            };
        }
        let result = match command.action {
            VendorAction::Reserve { context, batch } => self
                .authorize_vendor_use(context, batch.vendor)
                .and_then(|()| self.reserve_vendor_lazy_stock(*batch))
                .map(VendorDecision::Reserved)
                .map_err(VendorCommandError::Lazy),
            VendorAction::Adopt {
                context,
                batch,
                receipt,
            } => self
                .authorize_vendor_use(context, batch.vendor)
                .and_then(|()| self.adopt_vendor_loaded_stock(*batch, receipt))
                .map(VendorDecision::Adopted)
                .map_err(VendorCommandError::Lazy),
            VendorAction::Confirm(receipt) => self
                .confirm_vendor_lazy_stock(receipt)
                .map(|()| VendorDecision::Confirmed)
                .map_err(VendorCommandError::Lazy),
            VendorAction::ReserveDefaultBuy {
                context,
                vendor,
                source,
                requests,
                prepared,
                operation_id,
            } => self
                .reserve_vendor_default_buy(
                    context,
                    vendor,
                    source,
                    &requests,
                    &prepared,
                    operation_id,
                )
                .map(|ticket| VendorDecision::BuyReserved(Box::new(ticket)))
                .map_err(VendorCommandError::Buy),
            VendorAction::RejectBuy(ticket) => self
                .reject_vendor_buy(&ticket)
                .map(|()| VendorDecision::BuyRejected)
                .map_err(VendorCommandError::Buy),
            VendorAction::ConfirmBuy { ticket, receipt } => self
                .confirm_vendor_buy_committed(&ticket, &receipt)
                .map(VendorDecision::BuyConfirmed)
                .map_err(VendorCommandError::Buy),
        };
        VendorOutcome {
            correlation: command.correlation,
            result,
        }
    }

    fn adopt_vendor_loaded_stock(
        &mut self,
        batch: PreparedVendorLazyStock,
        receipt: VendorLazyStockReceipt,
    ) -> Result<VendorLazyStockTicket, G> {
        let ids: Vec<_> = batch
            .entries
            .iter()
            .flat_map(|entry| {
                std::iter::once(entry.tree.root).chain(entry.tree.items.iter().map(|item| item.id))
            })
            .collect();
        let base_revision = u64::try_from(batch.entries.len())
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or(G::Invalid)?;
        let purchase_count = receipt
            .marker_version
            .checked_sub(1)
            .and_then(|count| u64::try_from(count).ok())
            .ok_or(G::Stale)?;
        let marker_stock_revision = base_revision.checked_add(purchase_count).ok_or(G::Stale)?;
        if receipt.vendor != batch.vendor
            || receipt.marker != batch.marker
            || receipt.operation_id != batch.operation_id
            || batch.expected_stock_revision != 0
            || receipt.items.len() != ids.len()
            || receipt
                .items
                .iter()
                .zip(&ids)
                .any(|(&(actual, version), expected)| actual != *expected || version != 1)
        {
            return Err(G::Stale);
        }
        if let Some(owner) = self.generated_vendors.get(&batch.vendor)
            && owner.lazy_loaded
        {
            if owner.pending_buy.is_some() || owner.pending_lazy.is_some() {
                return Err(G::Busy);
            }
            let prior = owner.committed_lazy.as_ref().ok_or(G::Stale)?;
            if prior.vendor != batch.vendor
                || prior.marker != batch.marker
                || prior.source_revision != batch.source_revision
                || prior.source_hash != batch.source_hash
                || prior.item_ids != ids
                || prior.stock_revision != owner.stock.revision()
                || owner.marker_version != receipt.marker_version
                || owner.marker_stock_revision != marker_stock_revision
                || batch.entries.iter().any(|entry| {
                    owner.display_quantities.get(&entry.tree.root.0)
                        != Some(&entry.display_quantity)
                })
            {
                return Err(G::Stale);
            }
            return Ok(prior.clone());
        }
        let ticket = self.reserve_vendor_lazy_stock(batch)?;
        if ticket.stock_revision != base_revision {
            return Err(G::Stale);
        }
        let owner = self
            .generated_vendors
            .get_mut(&ticket.vendor)
            .ok_or(G::Missing)?;
        if owner
            .pending_lazy
            .as_ref()
            .is_none_or(|pending| pending.ticket != ticket)
        {
            return Err(G::Stale);
        }
        let pending = owner.pending_lazy.take().ok_or(G::Stale)?;
        owner.stock = pending.stock;
        owner.items = pending.items;
        owner.contents = pending.contents;
        owner.display_quantities = pending.display_quantities;
        owner.lazy_loaded = true;
        owner.committed_lazy = Some(pending.ticket);
        owner.marker_version = receipt.marker_version;
        owner.marker_stock_revision = marker_stock_revision;
        Ok(ticket)
    }

    pub(super) fn authorize_vendor_use(
        &mut self,
        context: bace_gameplay_api::ActionContext,
        vendor: EntityId,
    ) -> Result<(), G> {
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| G::Invalid)?;
        if self
            .world
            .combatant(context.actor)
            .is_none_or(|body| body.health() == 0)
        {
            return Err(G::Invalid);
        }
        let (actor_cell, actor) = self
            .world
            .actor_state(context.actor)
            .map_err(|_| G::Missing)?;
        let (vendor_cell, target) = self.world.actor_state(vendor).map_err(|_| G::Missing)?;
        let actor_shape = self
            .world
            .body(context.actor)
            .map_err(|_| G::Missing)?
            .collision_shape()
            .ok_or(G::Geometry)?;
        let vendor_shape = self
            .world
            .body(vendor)
            .map_err(|_| G::Missing)?
            .collision_shape()
            .ok_or(G::Geometry)?;
        let mut target_position = target.position();
        if actor_cell != vendor_cell {
            target_position = target_position
                + self
                    .world
                    .geometry()
                    .ok_or(G::Geometry)?
                    .frame_offset(actor_cell.0, vendor_cell.0)
                    .map_err(|_| G::Geometry)?;
        }
        let cylinder = |position: bace_geometry::Vec3, shape: &bace_physics::CollisionShape| {
            bace_inventory::InventoryCylinder {
                position: [position.x, position.y, position.z],
                radius: shape.nominal_radius().unwrap_or(shape.horizontal_radius()),
                height: shape.nominal_height().unwrap_or(shape.height()),
            }
        };
        let distance = bace_inventory::inventory_use_distance(
            cylinder(actor.position(), actor_shape),
            cylinder(target_position, vendor_shape),
        )
        .ok_or(G::Invalid)?;
        let radius = match self
            .world
            .properties(vendor)
            .and_then(|properties| properties.get(bace_entity::PropertyFamily::Float, 54))
        {
            Some(bace_entity::PropertyValue::Float(value))
                if value.is_finite() && *value > 0. && *value <= 20. =>
            {
                *value
            }
            Some(_) => return Err(G::Invalid),
            None => 0.6,
        };
        if distance > radius {
            return Err(G::Placement);
        }
        let from = actor.position() + bace_geometry::Vec3::new(0., 0., actor_shape.height() * 0.5);
        let to = target_position + bace_geometry::Vec3::new(0., 0., vendor_shape.height() * 0.5);
        if !self
            .world
            .segment_clear(actor_cell, from, to, Some(vendor.0))
            .map_err(|_| G::Geometry)?
        {
            return Err(G::Placement);
        }
        Ok(())
    }
    /// A static vendor may use the same bounded economy owner as a generated
    /// vendor. Duplicate registration never resets stock or an in-flight save.
    pub fn register_vendor_stock(&mut self, vendor: EntityId, capacity: usize) -> Result<(), G> {
        self.register_generator_vendor(vendor, capacity)
    }

    pub fn vendor_stock_display_quantity(&self, vendor: EntityId, item: EntityId) -> Option<i32> {
        self.generated_vendors
            .get(&vendor)?
            .display_quantities
            .get(&item.0)
            .copied()
    }

    pub fn reserve_vendor_lazy_stock(
        &mut self,
        batch: PreparedVendorLazyStock,
    ) -> Result<VendorLazyStockTicket, G> {
        if batch.vendor.0 == 0
            || batch.vendor_expected_version <= 0
            || batch.marker == batch.vendor
            || batch.source_revision == 0
            || batch.source_hash == [0; 32]
            || batch.operation_id.is_empty()
            || batch.operation_id.len() > 128
            || !batch
                .operation_id
                .bytes()
                .all(|byte| byte.is_ascii_graphic())
            || batch.entries.len() > 1024
            || !self.available_generator_id(batch.marker)
        {
            return Err(G::Invalid);
        }
        let owner = self
            .generated_vendors
            .get(&batch.vendor)
            .ok_or(G::Missing)?;
        // Existing generated stock needs a complete, source-fenced contribution
        // projection in the marker. Hold that mixed path until it exists.
        if owner.pending_lazy.is_some() || !owner.stock.items().is_empty() {
            return Err(G::Busy);
        }
        if owner.lazy_loaded || owner.stock.revision() != batch.expected_stock_revision {
            return Err(G::Stale);
        }
        let mut stock = owner.stock.clone();
        stock.mark_lazy_loaded().map_err(|_| G::Stale)?;
        let mut templates = owner.items.clone();
        let mut contents = owner.contents.clone();
        let mut display_quantities = owner.display_quantities.clone();
        let mut ids = BTreeSet::from([batch.marker]);
        let mut ordered = Vec::new();
        for entry in &batch.entries {
            let tree = &entry.tree;
            super::vendor_trees::validate_tree(tree)?;
            if !(-1..=0x00ff_ffff).contains(&entry.display_quantity)
                || entry.source_destinations.len() != tree.items.len() + 1
                || entry.source_destinations.get(&tree.root) != Some(&Some(4))
                || std::iter::once(&tree.template)
                    .chain(tree.templates.values())
                    .any(|source| matches!(source.weenie_type, 10 | 12 | 15 | 61 | 69 | 71))
            {
                return Err(G::Invalid);
            }
            for id in std::iter::once(tree.root).chain(tree.items.iter().map(|item| item.id)) {
                if !ids.insert(id)
                    || !self.available_generator_id(id)
                    || !entry.source_destinations.contains_key(&id)
                {
                    return Err(G::Invalid);
                }
                ordered.push(id);
            }
            let int = |id| {
                tree.template
                    .properties
                    .ints
                    .iter()
                    .find(|property| property.id == id)
                    .map(|property| property.value)
            };
            let item = VendorStockItem {
                id: tree.root.0,
                template: tree.template.weenie_id,
                stack: int(12),
                maximum_stack: int(11),
            };
            let proposal = stock.propose_lazy(item).map_err(|_| G::Invalid)?;
            stock.commit_lazy(proposal).map_err(|_| G::Invalid)?;
            templates.insert(tree.root.0, tree.template.clone());
            for (&id, source) in &tree.templates {
                templates.insert(id.0, source.clone());
            }
            contents.insert(
                tree.root.0,
                VendorContents {
                    items: tree.items.clone(),
                    containers: tree.containers.clone(),
                },
            );
            display_quantities.insert(tree.root.0, entry.display_quantity);
        }
        if ordered.len() > 1024 || templates.len() > 4096 {
            return Err(G::Capacity);
        }
        let ticket = VendorLazyStockTicket {
            vendor: batch.vendor,
            vendor_expected_version: batch.vendor_expected_version,
            marker: batch.marker,
            operation_id: batch.operation_id,
            source_revision: batch.source_revision,
            source_hash: batch.source_hash,
            expected_stock_revision: batch.expected_stock_revision,
            stock_revision: stock.revision(),
            item_ids: ordered,
        };
        self.generated_vendors
            .get_mut(&batch.vendor)
            .ok_or(G::Missing)?
            .pending_lazy = Some(Box::new(PendingVendorLazy {
            ticket: ticket.clone(),
            stock,
            items: templates,
            contents,
            display_quantities,
        }));
        Ok(ticket)
    }

    pub fn confirm_vendor_lazy_stock(&mut self, receipt: VendorLazyStockReceipt) -> Result<(), G> {
        let owner = self
            .generated_vendors
            .get_mut(&receipt.vendor)
            .ok_or(G::Missing)?;
        let ticket = owner
            .pending_lazy
            .as_ref()
            .map(|pending| &pending.ticket)
            .or(owner.committed_lazy.as_ref())
            .ok_or(G::Stale)?;
        if ticket.vendor != receipt.vendor
            || ticket.marker != receipt.marker
            || ticket.operation_id != receipt.operation_id
            || receipt.marker_version != 1
            || ticket.item_ids.len() != receipt.items.len()
            || ticket
                .item_ids
                .iter()
                .copied()
                .zip(&receipt.items)
                .any(|(expected, &(id, version))| expected != id || version != 1)
        {
            return Err(G::Stale);
        }
        let Some(pending) = owner.pending_lazy.take() else {
            return Ok(()); // Exact repeated receipt after adoption.
        };
        owner.stock = pending.stock;
        owner.items = pending.items;
        owner.contents = pending.contents;
        owner.display_quantities = pending.display_quantities;
        owner.lazy_loaded = true;
        owner.committed_lazy = Some(pending.ticket);
        owner.marker_version = receipt.marker_version;
        owner.marker_stock_revision = owner.stock.revision();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
